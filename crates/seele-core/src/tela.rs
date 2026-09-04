//! O transporte do compartilhamento de tela.
//!
//! Tudo aqui sai do §3 de
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`, que
//! é o único pedaço daquela spec **medido** antes de virar decisão. Os números
//! citados nos comentários abaixo são de `spikes/tela-no-transporte`, e o
//! `README` dele traz a tabela inteira.
//!
//! # As quatro decisões, e o que cada uma custou para descobrir
//!
//! 1. **O vídeo vai num fluxo unidirecional QUIC da conexão que já existe**
//!    ([`Transmissao::abrir`]). Não em datagrama: `send_datagram` põe voz e
//!    vídeo na **mesma fila FIFO** do `quinn-proto`, que descarta o mais velho
//!    quando enche — 16,1% da voz perdida e 2,16 s de atraso com o buffer
//!    padrão de 1 MiB, e **98,1% descartada** ao encolher o buffer para 32 KiB,
//!    porque aí os pedaços de vídeo enchem a fila entre dois quadros de voz.
//!    Com o vídeo num fluxo a perda de voz é 0,1%, e não é sorte: o
//!    `quinn-proto` escreve os quadros `DATAGRAM` **antes** dos `STREAM` em
//!    cada pacote. E não numa segunda conexão: duas conexões QUIC competem no
//!    mesmo gargalo, e a segunda devolve 4 ms por um aperto de mão inteiro.
//! 2. **O que protege a voz é o teto de banda, não o transporte**
//!    ([`TetoDeVideo`]). A prioridade do QUIC evita a perda e não faz nada
//!    contra o atraso: com o vídeo solto, a voz chega inteira e chega com
//!    225,7 ms em vez de 21,7, porque a fila de 262 ms do gargalo fica cheia o
//!    tempo todo — e essa fila não está nesta máquina, está no meio do
//!    caminho, onde prioridade de frame não alcança. O que alcança é não
//!    enchê-la.
//! 3. **O quadro-chave é metade do que sobra depois do teto**
//!    ([`Transmissao::enviar_quadro`]). Espalhar o **mesmo** quadro-chave em
//!    vez de despejá-lo num tique leva o p95 da voz de 78,9 para 35,8 ms e o
//!    pior caso de 114,9 para 42,7 ms, com o mesmo bitrate entregue. Custo:
//!    nenhum.
//! 4. **A voz nunca cede à tela.** Quem baixa resolução e quem para é o vídeo.
//!    É o critério de aceite do ciclo, e está escrito como teste — veja
//!    `a_voz_nunca_cede_a_tela` no fim deste arquivo.
//!
//! # Por que o enquadramento de quadro mora aqui e não em `seele-proto`
//!
//! O §3.6 da spec para no cabeçalho de abertura ([`seele_proto::screen`]) e não
//! diz o que separa um quadro do outro dentro do fluxo. Isto é essa peça, e ela
//! **deveria** morar ao lado do cabeçalho: é formato de fio, e formato de fio é
//! de `seele-proto`. Está aqui porque a tarefa que a escreveu não é dona
//! daquele crate, e mudá-lo por fora seria decidir sozinho uma coisa de dois
//! donos. Mover é uma linha de `pub use` no dia em que alguém puder.
//!
//! # `quinn-proto` fica onde está
//!
//! §3.5, e vale independentemente de tela: **`quinn-proto` 0.11.17 aborta o
//! processo no primeiro datagrama que estoura o buffer de envio** — o caminho
//! de descarte desconta `payload_bytes` duas vezes, o `usize` dá a volta, e o
//! `expect` seguinte estoura. O `Cargo.lock` trava 0.11.16 de propósito. É o
//! caminho por onde a **voz** sai hoje: basta a subida sumir por dois segundos
//! para o processo morrer em vez de perder quadros. Não subir sem conferir se
//! foi consertado, e no dia de subir, um teste que encha o buffer de propósito.

use std::time::{Duration, Instant};

use seele_proto::screen::{ScreenError, ScreenHeader, SCREEN_HEADER_LEN};
use seele_proto::signal::SignalBand;
use seele_video::codec::{Cadencia, Resolucao};
use thiserror::Error;

// ---------------------------------------------------------------------------
// O teto de banda
// ---------------------------------------------------------------------------

/// Que fração do caminho medido o vídeo pode ocupar, em por cento.
///
/// **60, e é medida e não gosto.** Com o vídeo pedindo 1200 kbps num caminho de
/// 2000 — 60% —, a voz volta para 23,1 ms de p50 e 0% de perda; solto, ela vai
/// a 225,7 ms no mesmo cano. É o único ponto em que o spike viu a voz de volta à
/// linha de base, e o §8 pergunta 1 diz o que ainda não se sabe dele: Wi-Fi ruim
/// tem perda esporádica e atraso que anda sozinho, e nenhum dos dois estava na
/// prova.
///
/// O que sobra — os outros 40% — é [`TetoDeVideo::reserva_da_voz`], e é a razão
/// de este número ser uma fração e não um valor de configuração: um teto fixo
/// num caminho estreito é um teto que come a voz, e num caminho largo é um teto
/// que desperdiça tela.
pub const FRACAO_DO_CAMINHO: u32 = 60;

/// O caminho por onde se **começa** enquanto ninguém mediu, em bits por segundo.
///
/// **Isto era a resposta e passou a ser o ponto de partida.** O §8 pergunta 2 —
/// *«como se mede o caminho quando ninguém está enchendo?»* — ficou aberta
/// enquanto o produto só sabia medir o sinal da voz, que diz que está bom a
/// 40 kbps e não diz quanto cabe. A resposta acabou sendo curta: **enquanto a
/// tela transmite, alguém está enchendo, e é a tela.** Quem mede é a
/// [`crate::caminho::Sonda`], sobre os contadores que o `quinn` já mantém, e o
/// cabeçalho daquele módulo tem a conta inteira — inclusive por que a medida
/// tem de ser o que a janela **carregou**, e não o valor em que doeu.
///
/// O que fica aqui é de onde a sonda parte: o caminho sobre o qual as duas
/// provas rodaram, 2000 kbps de subida, que dá o teto de 1200 kbps que
/// `spikes/tela-no-codec` usou em todas as linhas. Continua sendo a única
/// suposição com número atrás, e continua valendo em três lugares —
/// [`TetoDeVideo::novo`], a perna de quem hospeda enquanto o `HostUplink` não
/// chega, e o primeiro instante de uma sessão, antes da primeira janela cheia.
pub const CAMINHO_DA_PROVA_BPS: u32 = 2_000_000;

/// Abaixo deste teto o compartilhamento **para**, em bits por segundo.
///
/// O §2 pede piso com nome: *«se o encoder não sustenta nem o piso, o
/// compartilhamento para, com motivo enumerado. Degradar para sempre é como um
/// instrumento falso: consultado justamente quando algo deu errado.»*
///
/// **De onde sai o número, e o que nele é extrapolação.** `spikes/tela-no-codec`
/// mediu 540p — o piso da lista que o §5 fechou — gastando 796 kbps a 30
/// quadros. A faixa automática desce até 5 quadros (`PISO_DE_QUADROS` do
/// `seele-video`), e a conta ingênua daria 796/6 ≈ 133 kbps. Ingênua porque bits
/// não escalam com quadros: o quadro-chave custa o mesmo e o conteúdo parado
/// também. 200 kbps é essa conta com margem, e **não foi medida** — nenhuma
/// linha do spike rodou abaixo de 1200 kbps de teto. É o número que fica até
/// alguém medir; o que não pode faltar é o piso existir.
pub const PISO_DE_BANDA_BPS: u32 = 200_000;

/// Por que o vídeo não está saindo.
///
/// Enumerado, como `specs/02-protocolo.md` manda: quem recebe isto tem de poder
/// escrever a frase na língua da pessoa, e uma string de erro não deixa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MotivoDeParada {
    /// O sinal da voz caiu para [`SignalBand::Critical`].
    ///
    /// §3.2: *«quando o sinal cai de faixa, quem baixa é o vídeo; se continuar
    /// caindo, quem para é o vídeo»*. Uma conversa com a tela travando é o
    /// produto funcionando; uma conversa picotando porque alguém abriu a tela é
    /// o produto quebrado.
    #[error("the voice signal is critical; the screen gives way")]
    SinalCritico,
    /// O que sobrou do caminho não sustenta nem [`PISO_DE_BANDA_BPS`].
    #[error("the ceiling fell below the {PISO_DE_BANDA_BPS}-bps floor")]
    AbaixoDoPiso,
}

/// O teto de banda do vídeo neste instante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Teto {
    /// Pode transmitir, a no máximo estes bits por segundo.
    Bps(u32),
    /// Não pode transmitir, e a razão é dizível.
    Parado(MotivoDeParada),
}

impl Teto {
    /// Os bits por segundo, ou zero quando o vídeo está parado.
    ///
    /// Existe para a aritmética de quem compara tetos. **Não** serve para
    /// decidir se transmite: zero e parado são a mesma conta e frases
    /// diferentes, e é a frase que a pessoa lê.
    #[must_use]
    pub const fn bps(self) -> u32 {
        match self {
            Self::Bps(bps) => bps,
            Self::Parado(_) => 0,
        }
    }

    /// A resolução que este teto compra, ou `None` quando o vídeo está parado.
    ///
    /// `None` e não «540p»: parado não é a menor resolução, é resolução
    /// nenhuma. Devolver o degrau mais baixo aqui faria a interface desenhar um
    /// retângulo de 960×540 para uma transmissão que não existe.
    #[must_use]
    pub const fn resolucao_estimada(self) -> Option<Resolucao> {
        match self {
            Self::Bps(bps) => Some(resolucao_estimada_para(bps)),
            Self::Parado(_) => None,
        }
    }
}

/// Qual perna do `min` do §5.1 está mandando agora.
///
/// Existe para a tela, e o §5.1 a obriga: *«quando aperta, a tela diz que
/// apertou e por quê. Um compartilhamento que cai de resolução porque entrou a
/// quinta pessoa e não explica é o produto sabendo algo que quem está na frente
/// dele não sabe.»* O gatilho é o teto, mas a **razão** é isto — é a diferença
/// entre `720p · 6 pessoas assistindo` e `720p · sua conexão`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PernaQueAperta {
    /// A subida de quem hospeda, dividida pelos espectadores. É a perna que o
    /// §5.1 acrescentou, e a única que anda quando alguém entra ou sai.
    QuemHospeda,
    /// A subida de quem compartilha.
    QuemCompartilha,
    /// A escolha da pessoa (§5), que é sempre teto e nunca piso — apertar por
    /// aqui é o produto obedecendo, não o produto degradando.
    Escolha,
}

/// O teto do vídeo, pendurado no sinal que a voz já calcula.
///
/// § 3.2, regra 2: *«quem mede o caminho é a voz, que já mede»*. O produto
/// calcula RTT, jitter e perda por conexão e os transforma em Taxa de
/// Sincronização (ADR 0024); o teto do vídeo pendura nesse número em vez de
/// abrir um segundo medidor que discordaria do primeiro no primeiro dia ruim.
///
/// # As três pernas, e a que faltava
///
/// Decidido em 22/08/2026 (§5.1): **o servidor encaminha**, como ele já faz com
/// a voz em `voice_room::VoiceRoom::forward`. Alguém sobe N cópias — multicast não existe
/// na internet aberta, e um quadro que quatro pessoas assistem sai quatro vezes
/// de alguma máquina —, e nesta decisão essa máquina é a de **quem hospeda**.
/// Então o teto é o menor de três:
///
/// ```text
/// teto = min(
///     caminho de quem HOSPEDA × 60% ÷ N espectadores,   ← o que o servidor sobe
///     caminho de quem COMPARTILHA × 60%,                ← o que a fonte sobe
///     o que a pessoa escolheu (§5),                     ← sempre teto, nunca piso
/// )
/// ```
///
/// **A primeira linha é nova, e sem ela o produto media uma perna e estourava a
/// outra** — o §5.1 chama isso de «o defeito mais caro desta seção». Com quatro
/// espectadores a 1,2 Mbps são 4,8 Mbps saindo da casa de quem hospeda, mais a
/// voz de todo mundo, o que numa subida doméstica brasileira típica é mais do
/// que existe.
///
/// Quando quem compartilha **é** quem hospeda, as duas primeiras pernas são a
/// mesma máquina e quem chama passa o mesmo número nas duas. Esta estrutura não
/// tenta descobrir isso sozinha: ela não sabe quem é quem, e adivinhar seria
/// inventar a perna mais cara da conta.
///
/// # O tempo não entra aqui
///
/// Nada nesta estrutura lê relógio nem guarda histórico: ela é uma conta sobre
/// o que se sabe agora. Quem suaviza é a própria [`seele_proto::signal`],
/// com o α ≈ 0,2 que `specs/02-protocolo.md` fixa — suavizar duas vezes seria
/// pôr o teto atrás do sinal que ele existe para seguir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TetoDeVideo {
    caminho_bps: u32,
    caminho_de_quem_hospeda_bps: u32,
    espectadores: u32,
    escolha_bps: Option<u32>,
}

impl Default for TetoDeVideo {
    fn default() -> Self {
        Self::novo()
    }
}

/// [`FRACAO_DO_CAMINHO`] por cento de um caminho, sem dar a volta.
///
/// Em `u64` porque `caminho_bps × 60` estoura `u32` a partir de uns 71 Mbit/s,
/// que é uma fibra doméstica comum. Um teto que dá a volta vira um teto
/// minúsculo, e o defeito apareceria só na casa boa.
const fn fracao_do(caminho_bps: u32) -> u32 {
    ((caminho_bps as u64 * FRACAO_DO_CAMINHO as u64) / 100) as u32
}

impl TetoDeVideo {
    /// O teto de quem ainda não mediu caminho nenhum: o cano da prova nas duas
    /// pernas, e um espectador.
    ///
    /// **Um, e não zero**, porque uma transmissão sem ninguém assistindo não
    /// tem razão de existir — e porque com o piso de `copias` os dois
    /// dão exatamente a mesma conta, então o número que fica escrito aqui é o
    /// que descreve o caso de uso e não o que economiza uma linha.
    #[must_use]
    pub const fn novo() -> Self {
        Self {
            caminho_bps: CAMINHO_DA_PROVA_BPS,
            caminho_de_quem_hospeda_bps: CAMINHO_DA_PROVA_BPS,
            espectadores: 1,
            escolha_bps: None,
        }
    }

    /// O teto sobre o caminho de subida de **quem compartilha**, em bits por
    /// segundo.
    ///
    /// **O número vem da [`crate::caminho::Sonda`]**, que o mede enquanto a tela
    /// enche o cano — era a pergunta 2 do §8, e este construtor foi escrito para
    /// o dia em que ela tivesse resposta. Quem chama são
    /// [`crate::state::Room::teto_de_video`] e o motor do `crate::enlace`, e o
    /// que a sonda devolve antes da primeira janela cheia é exatamente
    /// [`CAMINHO_DA_PROVA_BPS`] — de modo que a primeira transmissão de uma
    /// sessão abre com o mesmo teto de sempre.
    #[must_use]
    pub const fn com_caminho(caminho_bps: u32) -> Self {
        Self {
            caminho_bps,
            caminho_de_quem_hospeda_bps: CAMINHO_DA_PROVA_BPS,
            espectadores: 1,
            escolha_bps: None,
        }
    }

    /// O caminho de subida de **quem hospeda**, que é por onde as N cópias
    /// saem (§5.1).
    ///
    /// Fica separado de [`Self::com_caminho`] porque são duas máquinas
    /// diferentes com duas medidas diferentes, e juntá-las num campo só foi
    /// exatamente o defeito que o §5.1 mandou corrigir.
    #[must_use]
    pub const fn com_caminho_de_quem_hospeda(mut self, caminho_bps: u32) -> Self {
        self.caminho_de_quem_hospeda_bps = caminho_bps;
        self
    }

    /// Quantas pessoas estão assistindo, que é quantas cópias o servidor sobe.
    ///
    /// **Zero é estado normal**, e o §5.1 não o trata: uma transmissão que
    /// começou antes de alguém abrir a tela, ou de onde o último espectador
    /// acabou de sair. Zero não pode ser divisão por zero e também não pode ser
    /// teto infinito — ver `copias`.
    #[must_use]
    pub const fn com_espectadores(mut self, espectadores: u32) -> Self {
        self.espectadores = espectadores;
        self
    }

    /// A escolha de quem compartilha, que é **teto e nunca piso** (§5).
    ///
    /// A regra que não se negocia: o que a pessoa escolhe é o máximo, e o
    /// sistema continua livre para ficar abaixo. Se virasse piso, a regra de
    /// aceite do §3.2 cairia — alguém escolhe 1080p60 numa subida de 2 Mbps, o
    /// vídeo insiste, e a conversa fica impossível **por causa da tela**. Aí o
    /// produto fica pior com o recurso do que sem ele.
    #[must_use]
    pub const fn com_escolha(mut self, escolha_bps: Option<u32>) -> Self {
        self.escolha_bps = escolha_bps;
        self
    }

    /// Quantas pessoas estão assistindo, como quem chamou informou.
    ///
    /// Devolve o número **cru**, e não o de `copias`: quem escreve
    /// `720p · 6 pessoas assistindo` na tela (§5.1) precisa do que é verdade,
    /// não do que a aritmética usou para não dividir por zero.
    #[must_use]
    pub const fn espectadores(&self) -> u32 {
        self.espectadores
    }

    /// Por quanto a perna de quem hospeda é dividida.
    ///
    /// **Zero espectador vira uma cópia, e isso é uma escolha.** Dividir por
    /// zero não existe; não dividir — ou seja, deixar a perna passar inteira —
    /// seria um teto que sobe justamente quando ninguém está olhando, e a
    /// primeira pessoa a entrar o derrubaria de volta com um salto que a
    /// interface teria de explicar. Uma cópia é o menor número de cópias que
    /// uma transmissão viva chega a subir, e é o valor que faz a conta ser
    /// contínua na entrada da primeira pessoa.
    const fn copias(&self) -> u32 {
        if self.espectadores == 0 {
            1
        } else {
            self.espectadores
        }
    }

    /// O que fica para a voz **desta máquina**, em bits por segundo, aconteça o
    /// que acontecer.
    ///
    /// **Este número não depende da faixa nem de quanta gente está
    /// assistindo**, e é isso que a frase *«a voz nunca cede à tela»* quer dizer
    /// em aritmética: quando o sinal piora, ou quando a sala cresce, o que muda
    /// é o teto do vídeo. A reserva da voz é o que sobra do caminho de quem
    /// compartilha depois de [`FRACAO_DO_CAMINHO`] e não encolhe nunca.
    ///
    /// A mesma reserva existe do lado de quem hospeda, e não precisa de um
    /// segundo método: as N cópias saem de [`FRACAO_DO_CAMINHO`] daquele
    /// caminho **dividido por N**, então N × teto nunca passa dos 60% de lá e
    /// os outros 40% ficam para a voz de todo mundo que passa pelo servidor.
    #[must_use]
    pub const fn reserva_da_voz(&self) -> u32 {
        self.caminho_bps - self.perna_de_quem_compartilha()
    }

    /// 60% do caminho de quem compartilha: o que esta máquina pode subir.
    const fn perna_de_quem_compartilha(&self) -> u32 {
        fracao_do(self.caminho_bps)
    }

    /// 60% do caminho de quem hospeda, dividido pelas cópias que o servidor
    /// sobe. A perna que o §5.1 acrescentou.
    const fn perna_de_quem_hospeda(&self) -> u32 {
        fracao_do(self.caminho_de_quem_hospeda_bps) / self.copias()
    }

    /// O menor das duas pernas de rede, antes da faixa e da escolha.
    const fn teto_da_faixa_nominal(&self) -> u32 {
        let hospeda = self.perna_de_quem_hospeda();
        let compartilha = self.perna_de_quem_compartilha();
        if hospeda < compartilha {
            hospeda
        } else {
            compartilha
        }
    }

    /// Qual das três pernas está mandando nesta faixa (§5.1).
    ///
    /// Empate vai para a perna de quem hospeda e depois para a de quem
    /// compartilha, nesta ordem, porque é a ordem em que a rede manda: numa
    /// sala onde os três números se encontram, dizer «a escolha» seria dizer à
    /// pessoa que basta escolher mais.
    #[must_use]
    pub fn perna_que_aperta(&self, faixa: SignalBand) -> PernaQueAperta {
        let hospeda = self.perna_de_quem_hospeda();
        let compartilha = self.perna_de_quem_compartilha();
        // A faixa corta as duas pernas de rede pelo mesmo fator, então ela não
        // muda quem é a menor — só a escolha da pessoa é que pode ultrapassar
        // uma perna cortada, e é por isso que ela entra depois.
        let rede = if hospeda <= compartilha {
            (hospeda, PernaQueAperta::QuemHospeda)
        } else {
            (compartilha, PernaQueAperta::QuemCompartilha)
        };
        let da_faixa = match faixa {
            SignalBand::Nominal => rede.0,
            SignalBand::Degraded => rede.0 / 2,
            SignalBand::Critical => return rede.1,
        };
        match self.escolha_bps {
            Some(escolha) if escolha < da_faixa => PernaQueAperta::Escolha,
            _ => rede.1,
        }
    }

    /// O teto agora, dada a faixa em que o sinal da voz está.
    ///
    /// As três saídas são as três frases do §3.2, nesta ordem:
    ///
    /// - [`SignalBand::Nominal`] — o vídeo tem [`FRACAO_DO_CAMINHO`] do caminho;
    /// - [`SignalBand::Degraded`] — **quem baixa é o vídeo**, e baixa pela
    ///   metade. A metade não foi medida: o que foi medido é o 60% na faixa
    ///   nominal. Metade é o menor passo que ainda **é** um passo — um corte de
    ///   10% seria indistinguível do ruído do próprio encoder, que já descarta
    ///   16% dos quadros em 1080p por conta própria;
    /// - [`SignalBand::Critical`] — **quem para é o vídeo**, com motivo.
    ///
    /// E, por baixo das três, a escolha da pessoa e o piso.
    #[must_use]
    pub fn teto(&self, faixa: SignalBand) -> Teto {
        let nominal = self.teto_da_faixa_nominal();
        let da_faixa = match faixa {
            SignalBand::Nominal => nominal,
            SignalBand::Degraded => nominal / 2,
            SignalBand::Critical => return Teto::Parado(MotivoDeParada::SinalCritico),
        };
        // O mínimo entre o que os dois caminhos aguentam e o que a pessoa
        // pediu. Os três são teto; quem manda é o menor, sempre.
        let teto = match self.escolha_bps {
            Some(escolha) => da_faixa.min(escolha),
            None => da_faixa,
        };
        if teto < PISO_DE_BANDA_BPS {
            return Teto::Parado(MotivoDeParada::AbaixoDoPiso);
        }
        Teto::Bps(teto)
    }
}

// ---------------------------------------------------------------------------
// A resolução, que acompanha o teto e não a contagem de gente
// ---------------------------------------------------------------------------

/// A partir de quanto o orçamento compra 1080p, em bits por segundo.
///
/// **`ESTIMADO` está no nome de propósito, e não sai até alguém medir.** O
/// §5.1 escreve isso com todas as letras: *«a tabela mede um teto só. Os
/// limiares certos saem de uma corrida por teto — 1200, 800, 500, 300 kbps —, e
/// ela ainda não foi feita.»*
///
/// De onde este número veio, e o que nele é chute:
///
/// | resolução | kbps entregues | quadros perdidos |
/// |---|---|---|
/// | 1080p | 1146 | **16,2%** |
/// | 720p | 872 | 11,1% |
/// | 540p | 796 | 12,4% |
/// | 360p | 416 | 2,2% |
///
/// A única medida que existe é essa, e ela é **num teto só**, o de 1200 kbps
/// que `spikes/tela-no-transporte` sustenta. O que ela diz sobre 1080p é
/// negativo: a 1200 kbps o controle de taxa do OpenH264 joga fora um sexto do
/// que a captura entrega, e o que chega do outro lado é imagem grande e trêmula.
/// Ela **não** diz onde 1080p passa a caber.
///
/// **6,2 Mbps, e o número saiu da tabela para uma conta.** Este limiar valeu
/// 1500 kbps, e aquele valor era o palpite de *onde o OpenH264 pararia de jogar
/// quadro fora* — 1146 kbps entregues cobrindo 83,8% dos quadros, e uns
/// 1368 kbps para carregar o resto. É o ponto em que o codificador para de
/// passar fome, não o ponto em que a imagem fica boa, e os dois estão longe um
/// do outro: 1080p a 1500 kbps e 30 quadros são **0,024 bits por pixel**.
///
/// Tela com texto pede cerca de **0,10 bits por pixel** — é aí que a borda de
/// uma fonte para de virar bloco. 1080p tem 2 073 600 pixels, então a 30
/// quadros a conta é `2 073 600 × 0,10 × 30 = 6,2 Mbps`, e é só isso que este
/// número é.
///
/// **O que a mudança custa, e é visível:** quem via «1080p» nas conexões de
/// 2 Mbps passa a ver «720p». Não é regressão — era um 1080p de 0,024 bpp, que
/// é o relato «a imagem fica borrada e blocada». Mas o §5 manda mostrar o que
/// está saindo, então a troca aparece na interface e a nota de versão precisa
/// dizer por quê.
///
/// **O que ela não resolve:** 540p num monitor grande mostrando código continua
/// ilegível por mais nítido que seja. Resolução também é legibilidade, e bits
/// por pixel não captura isso — é a razão de este número ser calibrado por
/// medida **e** por alguém olhando o texto, e não só por PSNR.
///
/// # Por que é derivado e não escrito
///
/// A invariante que este limiar tem de cumprir está escrita como teste desde
/// antes desta calibração: *«nos limiares, a cadência cheia cabe»*. Ela só vale
/// se o limiar for **exatamente** [`bits_por_quadro`] vezes
/// [`CADENCIA_DE_REFERENCIA`] — e a primeira versão desta mudança escreveu
/// 6 200 000 à mão, 10 000 abaixo do que a divisão de [`cadencia_para`] precisa,
/// o que pôs 1080p a 15 quadros em cima do próprio limiar. Duas constantes
/// arredondadas em separado divergem; uma derivada da outra não pode.
pub const TETO_ESTIMADO_PARA_1080P_BPS: u32 =
    bits_por_quadro(Resolucao::P1080) * CADENCIA_DE_REFERENCIA;

/// A cadência contra a qual os limiares de resolução são medidos.
///
/// **30, e é o padrão do §5.** Um limiar de resolução é uma frase sobre bits por
/// pixel, e bits por pixel só existem depois de alguém dizer a quantos quadros.
/// Este é esse alguém: os limiares dizem «esta resolução cabe **a 30 quadros**»,
/// e é [`cadencia_para`] quem trata dos outros degraus a partir do mesmo
/// [`bits_por_quadro`].
pub const CADENCIA_DE_REFERENCIA: u32 = 30;

/// A partir de quanto o orçamento compra 720p, em bits por segundo.
///
/// Mesma tabela, mesma ressalva de [`TETO_ESTIMADO_PARA_1080P_BPS`]: um teto
/// medido, e o resto é estimativa.
///
/// **2,8 Mbps, pela mesma conta de [`TETO_ESTIMADO_PARA_1080P_BPS`].** Este
/// limiar valeu 900 kbps, que era o que 720p **de fato gastou** no teto de 1200
/// — e gastar não é o mesmo que precisar: naquele ponto o codificador entregava
/// 0,033 bits por pixel a 30 quadros, um terço do que texto pede.
///
/// 720p tem 921 600 pixels: `921 600 × 0,10 × 30 = 2,76 Mbps`.
///
/// **Derivado e não escrito**, pela mesma razão de
/// [`TETO_ESTIMADO_PARA_1080P_BPS`]: dois arredondamentos independentes já
/// tinham quebrado a invariante uma vez.
pub const TETO_ESTIMADO_PARA_720P_BPS: u32 =
    bits_por_quadro(Resolucao::P720) * CADENCIA_DE_REFERENCIA;

/// A maior resolução que este teto ainda compra (§5.1).
///
/// **O gatilho é o teto, e não a contagem de gente.** O pedido de quem desenha
/// o produto era «se tiver mais que 4 pessoas vai para 720p, 10 vai para 480p»,
/// e o §5.1 aceita a intenção e recusa o gatilho pelo motivo que o §5 já
/// escreve: *resolução não controla tráfego*. Dez pessoas numa fibra cabem em
/// 1080p; quatro numa subida ruim não cabem em 720p. Amarrar a resolução à
/// contagem degradaria a primeira à toa e ainda estouraria a segunda.
///
/// A contagem entra assim mesmo, e entra pelo lugar certo: N está **dentro** do
/// teto, pela perna de quem hospeda ([`TetoDeVideo`]). Quem quiser escrever
/// `720p · 6 pessoas assistindo` na tela tem as duas metades —
/// [`TetoDeVideo::espectadores`] para o número e
/// [`TetoDeVideo::perna_que_aperta`] para a razão.
///
/// O resultado é **teto**, como tudo no §5: quem escolheu 540p continua em
/// 540p num caminho de fibra. Combinar as duas escolhas é
/// [`menor_resolucao`].
/// O que cede primeiro quando o orçamento aperta.
///
/// # Por que isto é uma escolha, e não uma regra
///
/// O §2 fixou uma: *«a resolução segura, o quadro cede — texto continua legível
/// a 8 quadros e vira borrão no instante em que se reduz a resolução»*. Está
/// certo, e está certo **para texto**, que é o conteúdo que a spec tinha em
/// mente.
///
/// Jogo quer o contrário, e a diferença não é de grau: a 8 quadros um jogo não
/// é «pior», é inutilizável, enquanto a mesma partida a 540p continua sendo
/// jogável. Medido em campo entre um Mac e um Windows em LAN — «imagens muito
/// pixeladas» —, que é a regra do §2 aplicada ao conteúdo para o qual ela não
/// foi escrita.
///
/// Então a regra vira eixo, e quem compartilha escolhe. O padrão continua sendo
/// o do §2, porque compartilhar tela ainda é, na maioria das vezes, mostrar uma
/// tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prioridade {
    /// A resolução segura, o quadro cede. Texto, apresentação, código.
    ///
    /// O padrão, e o §2.
    #[default]
    Nitidez,
    /// O quadro segura, a resolução cede. Jogo, vídeo, qualquer coisa que mexe.
    Movimento,
}

/// A resolução que este teto compra, dado o que se escolheu proteger.
#[must_use]
pub const fn resolucao_para(teto_bps: u32, prioridade: Prioridade) -> Resolucao {
    match prioridade {
        Prioridade::Nitidez => resolucao_estimada_para(teto_bps),
        // **Limiares mais altos, e não um degrau abaixo do que couber.**
        //
        // A versão anterior descia um degrau, sempre. Ela cumpria a intenção
        // quando o teto apertava e a estragava quando não apertava: a 50 Mbps
        // ela continuava descendo, e por isso `Movimento` **nunca alcançava
        // 1080p, em banda nenhuma**. Como ele virou o padrão ao sair da caixa
        // de compartilhar, 1080p passou a ser inalcançável no produto inteiro —
        // e foi assim que a pergunta chegou: «se eu quero transmitir em 1080p60
        // para 5-6 pessoas, fica impossível então?».
        //
        // O que a intenção quer dizer é «para movimento, a mesma resolução
        // custa mais», e isso é um limiar e não um degrau. O dobro, e a razão é
        // a mesma tabela que os limiares de nitidez usam: ela mediu conteúdo
        // parado, e conteúdo em movimento gasta cerca do dobro por quadro
        // porque a predição entre quadros para de acertar.
        //
        // O efeito nas pontas é o certo: a 1,2 Mbps `Movimento` continua dando
        // 540p, exatamente como antes; a 8 Mbps ele passa a dar 1080p, que
        // antes era impossível.
        Prioridade::Movimento => resolucao_para_movimento(teto_bps),
    }
}

/// Quantos bits um quadro desta resolução precisa para não sair borrado.
///
/// # De onde estes números vêm
///
/// Da **mesma tabela** que deu os limiares de [`resolucao_para`], lida pela
/// outra coluna. Ela mediu o OpenH264 num teto de 1200 kbps:
///
/// | resolução | kbps entregues | quadros perdidos | bits por quadro entregue |
/// |---|---|---|---|
/// | 1080p | 1146 | 16,2% | 45,6 k |
/// | 720p  |  872 | 11,1% | 32,7 k |
/// | 540p  |  796 | 12,4% | 30,3 k |
///
/// A coluna da direita é a que ninguém tinha usado, e ela é a mais importante
/// das três: é **quanto o codificador de referência gastou em cada quadro que
/// ele decidiu entregar**. Ele chegou àquela qualidade jogando um sexto dos
/// quadros fora — está escrito na tabela — e gastando os bits que sobraram nos
/// que ficaram.
///
/// # Por que isto faltava, e o que ele conserta
///
/// O §2 fixou a regra: *«a resolução segura, o quadro cede — texto continua
/// legível a 8 quadros e vira borrão no instante em que se reduz a
/// resolução»*. A metade da resolução foi implementada em [`resolucao_para`];
/// **a metade do quadro nunca foi**. A cadência era a escolha de quem
/// compartilha, 30 por padrão, e nada a reduzia quando o orçamento apertava.
///
/// Isso funcionava por acidente enquanto o codificador era o do Cisco, que
/// jogava quadro fora sozinho. Com o codec do sistema deixou de funcionar, e de
/// formas opostas nos dois: o comentário de `codec/macos.rs` mediu 26,8 dB em 13
/// quadros de 120 contra **16,2 dB em 120 de 120** — o VideoToolbox entrega
/// todos os quadros e borra cada um, que é literalmente o «vira borrão» que a
/// regra do §2 existe para evitar.
///
/// Relatado assim: «assistindo a transmissão do mac, a imagem fica borrada e
/// blocada».
#[must_use]
pub const fn bits_por_quadro(resolucao: Resolucao) -> u32 {
    // **0,10 bits por pixel, e não o que o OpenH264 gastou.**
    //
    // A tabela anterior — 45 k a 1080p, 33 k a 720p, 30 k a 540p — era a coluna
    // da direita do quadro acima: o gasto por quadro **entregue** do
    // codificador de referência num teto de 1200 kbps. Naquele ponto ele estava
    // jogando 16,2% dos quadros fora. É o ponto de fome dele, não o ponto em
    // que a imagem fica boa, e adotá-lo como alvo fez o produto escolher sempre
    // mais quadros do que os bits pagavam — 45 k num quadro de 1080p são 0,022
    // bits por pixel.
    //
    // 0,10 bpp é onde a borda de uma fonte para de virar bloco. Os três números
    // abaixo são essa constante vezes os pixels de cada degrau, arredondados
    // **para cima**, e nada mais. Para cima e não para o mais próximo: 207 000
    // num quadro de 1080p são 0,0998 bpp, e um piso que o próprio piso não
    // alcança é um piso que não guarda nada — foi o que
    // `cada_limiar_compra_a_resolucao_que_promete` acusou.
    match resolucao {
        // 2 073 600 px × 0,10 = 207 360
        Resolucao::P1080 => 208_000,
        //   921 600 px × 0,10 = 92 160
        Resolucao::P720 => 93_000,
        //   518 400 px × 0,10
        Resolucao::P540 => 52_000,
    }
}

/// A cadência que este teto compra nesta resolução — a metade que faltava do §2.
///
/// `escolha` é teto, como tudo no §5: o que sai é o menor entre o que a pessoa
/// pediu e o que o orçamento carrega. Quem escolheu 8 quadros continua em 8 numa
/// fibra.
///
/// # Movimento não cede quadro, e é o eixo inteiro
///
/// Para [`Prioridade::Movimento`] a escolha sai intacta. Não é exceção: é a
/// definição daquele eixo — «o quadro segura, a resolução cede» —, e a
/// resolução já cedeu em [`resolucao_para_movimento`], que cobra o dobro por
/// degrau. Cortar quadro aqui cobraria duas vezes pela mesma falta de banda, e
/// a 8 quadros um jogo não é pior: é inutilizável.
#[must_use]
pub fn cadencia_para(
    teto_bps: u32,
    resolucao: Resolucao,
    prioridade: Prioridade,
    escolha: Cadencia,
) -> Cadencia {
    // **Movimento não cede quadro por qualidade, e cede por transporte.**
    //
    // A regra do eixo continua valendo — «o quadro segura, a resolução cede», e
    // a resolução já cedeu em `resolucao_para_movimento`, que cobra o dobro por
    // degrau. Cortar quadro aqui por gosto cobraria duas vezes pela mesma falta
    // de banda, e é o que `movimento_nao_paga_duas_vezes_pela_mesma_falta_de_banda`
    // proíbe.
    //
    // O que este piso guarda é outra coisa, e ela não é negociável: **abaixo de
    // um certo orçamento por quadro o codificador do sistema para de respeitar
    // o teto.** Medido no VideoToolbox, conteúdo realista, em
    // `seele-video/tests/qualidade-do-codec.rs`:
    //
    // ```text
    // 0,010 a 0,048 bits/pixel → entregou 118% a 157% do teto
    // 0,077 bits/pixel e acima → entregou 91% a 92%
    // ```
    //
    // Ele tem um piso de qualidade interno e, quando o orçamento exige ir abaixo
    // dele, fura o teto em vez de obedecer. E um teto furado não é um defeito de
    // imagem: é o §3.2 caindo — o fluxo congestiona, a fila do gargalo enche, e
    // a voz vai de 23 para 225 ms. Ceder quadro aqui não é preferência, é a
    // única maneira de manter a promessa.
    //
    // Três quartos do piso de nitidez, e o número é o ponto medido **limpo**:
    // entre 0,048 e 0,077 ninguém mediu, e escolher a borda de baixo seria
    // escolher o lado errado de uma faixa desconhecida.
    let piso = if matches!(prioridade, Prioridade::Movimento) {
        bits_por_quadro(resolucao) / 4 * 3
    } else {
        bits_por_quadro(resolucao)
    };
    let cabe = teto_bps / piso;
    let maior = Cadencia::TODAS
        .into_iter()
        .rev()
        .find(|degrau| degrau.hz() <= cabe)
        // Abaixo do menor degrau não há cadência a escolher. Quem manda parar é
        // o piso de banda do §2, em `TetoDeVideo`, e não esta função: aqui o
        // menor degrau é o menor degrau.
        .unwrap_or(Cadencia::Q8);
    if maior.hz() <= escolha.hz() {
        maior
    } else {
        escolha
    }
}

/// A escada de [`Prioridade::Movimento`]: os mesmos degraus, o dobro do preço.
///
/// Separada de [`resolucao_estimada_para`] em vez de ser um degrau por cima
/// dela, para que os dois eixos sejam duas tabelas e não uma tabela e uma
/// correção — uma correção não tem como dizer «a 50 Mbps não corrija nada».
#[must_use]
pub const fn resolucao_para_movimento(teto_bps: u32) -> Resolucao {
    if teto_bps >= TETO_ESTIMADO_PARA_1080P_BPS * 2 {
        Resolucao::P1080
    } else if teto_bps >= TETO_ESTIMADO_PARA_720P_BPS * 2 {
        Resolucao::P720
    } else {
        Resolucao::P540
    }
}

/// A resolução que este teto compra pela regra do §2 — a resolução segura.
///
/// É a metade «nitidez» de [`resolucao_para`], e continua pública porque é ela
/// que carrega os limiares medidos: quem for remedi-los mexe aqui, e o eixo de
/// [`Prioridade`] se ajusta sozinho por cima.
#[must_use]
pub const fn resolucao_estimada_para(teto_bps: u32) -> Resolucao {
    if teto_bps >= TETO_ESTIMADO_PARA_1080P_BPS {
        Resolucao::P1080
    } else if teto_bps >= TETO_ESTIMADO_PARA_720P_BPS {
        Resolucao::P720
    } else {
        // 540p é o piso da lista do §5, e abaixo dele não há degrau: quem quer
        // gastar menos internet mexe no teto de banda, que é o controle
        // desenhado para isso. Se nem 540p couber, quem para é o piso de
        // [`PISO_DE_BANDA_BPS`], com motivo enumerado — não uma quarta
        // resolução.
        Resolucao::P540
    }
}

/// A menor de duas resoluções.
///
/// É como a escolha da pessoa e o degrau do teto se combinam: os dois são teto
/// (§5), e quem manda é o menor. Comparar por área, e não pela ordem da
/// enumeração, porque a ordem de uma enumeração é uma coisa que alguém
/// reorganiza sem perceber que decidiu algo.
#[must_use]
pub const fn menor_resolucao(a: Resolucao, b: Resolucao) -> Resolucao {
    if a.largura() * a.altura() <= b.largura() * b.altura() {
        a
    } else {
        b
    }
}

// ---------------------------------------------------------------------------
// O orçamento de bytes do fluxo
// ---------------------------------------------------------------------------

/// O orçamento de bytes de uma transmissão, em balde de fichas.
///
/// **Não é o limitador do vídeo — é a rede de segurança dele.** Quem faz o
/// vídeo caber no teto é o controle de taxa do OpenH264, e ele é bom nisso: no
/// teto de 1200 kbps ele descarta 16% dos próprios quadros em 1080p para não
/// estourar. Este balde existe para o caso em que ele estoura assim mesmo,
/// porque um byte escrito num fluxo QUIC é um byte na fila do gargalo, e a fila
/// do gargalo é exatamente o que o §3.2 mediu custando 200 ms de voz.
///
/// Duplicado do `crate::taxa::Balde` do `seele-server` de propósito, como
/// `crate::frame` é duplicado: o ADR 0002 impede o cliente e o daemon de
/// dividirem um crate de transporte, e quarenta linhas de balde custam menos
/// que um crate que os dois dependeriam e nenhum seria dono.
///
/// O tempo entra por parâmetro, como em [`crate::battery`] e em
/// `seele_server::taxa`: é o que torna testável o comportamento no limite sem
/// um único `sleep`.
#[derive(Debug, Clone, Copy)]
struct Balde {
    /// Rajada máxima, em bytes.
    capacidade: f64,
    /// Reposição, em bytes por segundo.
    por_segundo: f64,
    fichas: f64,
    ultima: Instant,
}

impl Balde {
    /// Um balde cheio, dimensionado para um teto em bits por segundo.
    ///
    /// Cheio, e não vazio: o primeiro quadro de uma transmissão não deve
    /// esperar. Capacidade de um segundo de orçamento porque é a unidade em que
    /// o teto é dito, e porque um quadro-chave de 1080p — 65 KiB, o maior que
    /// `spikes/tela-no-codec` mediu — cabe em menos da metade dela a 1200 kbps.
    fn novo(teto_bps: u32, agora: Instant) -> Self {
        let por_segundo = f64::from(teto_bps) / 8.0;
        Self {
            capacidade: por_segundo,
            por_segundo,
            fichas: por_segundo,
            ultima: agora,
        }
    }

    fn repor(&mut self, agora: Instant) {
        let decorrido = agora.saturating_duration_since(self.ultima).as_secs_f64();
        self.fichas = (self.fichas + decorrido * self.por_segundo).min(self.capacidade);
        self.ultima = agora;
    }

    /// Gasta `bytes` fichas, se houver todas. Tudo ou nada: meio quadro
    /// autorizado não é autorização nenhuma, e o outro meio não tem para onde
    /// ir num fluxo ordenado.
    fn gastar(&mut self, bytes: usize, agora: Instant) -> bool {
        self.repor(agora);
        let custo = bytes as f64;
        if self.fichas >= custo {
            self.fichas -= custo;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// O enquadramento de quadro dentro do fluxo
// ---------------------------------------------------------------------------

/// Bytes de cabeçalho na frente de cada quadro codificado.
///
/// Um byte de tipo e quatro de tamanho. Ao contrário do
/// [`SCREEN_HEADER_LEN`], que sai uma vez por transmissão, este sai trinta
/// vezes por segundo: cinco bytes são 150 B/s, que somem ao lado dos 150 kB/s
/// do teto, e valem o que compram — um receptor que sabe onde o quadro acaba
/// sem parsear NAL nenhuma.
pub const CABECALHO_DE_QUADRO_LEN: usize = 5;

/// Maior quadro codificado que este build carrega, em bytes.
///
/// `specs/08-seguranca.md`: o tamanho é conferido **antes** de qualquer
/// alocação. Ler um tamanho de 4 GiB de um par e reservar por ele é a negação
/// de serviço mais velha que existe.
///
/// 512 KiB é oito vezes o maior quadro que alguém mediu — o quadro-chave de
/// 1080p, 65 KiB em `spikes/tela-no-codec`. Folga para o encoder ter um dia
/// ruim, e ainda três ordens de grandeza abaixo do que dói.
pub const MAX_QUADRO_LEN: usize = 512 * 1024;

/// Em quantas fatias um quadro-chave sai (§3.3).
///
/// Quatro, e o número vem da forma da medida e não de gosto: o quadro-chave de
/// 1080p é **quatro vezes** um quadro comum (65 KiB contra ~5 KiB no teto de
/// 1200 kbps), então quatro fatias é o que o faz caber no mesmo tique que um
/// quadro comum cabe. Espalhar assim leva o p95 da voz de 78,9 para 35,8 ms e o
/// pior caso de 114,9 para 42,7 ms, **com o mesmo bitrate entregue**: não se
/// manda menos, manda-se em quatro tiques.
pub const FATIAS_DO_QUADRO_CHAVE: usize = 4;

/// Prioridade do fluxo de tela, abaixo de tudo o mais que este cliente escreve.
///
/// O controle é 1 e as transferências são −1 (`crate::client`). A tela é −2, e
/// a ordem importa menos do que parece: o §3.2 é explícito em que **prioridade
/// dentro do QUIC não alcança a fila do gargalo**, que é onde a voz sofre. Isto
/// só arruma a ordem de saída desta máquina, e o que arruma a voz é o teto.
pub const PRIORIDADE_DA_TELA: i32 = -2;

/// O que vem dentro de um quadro do fluxo de tela.
///
/// O byte de tipo era `u8::from(chave)` — zero ou um — e ganhou um terceiro
/// valor quando o som passou a viajar junto. Nomeado em vez de escrito à mão
/// porque agora são três lugares que precisam concordar: quem escreve aqui, quem
/// lê em [`Recepcao::proximo_quadro`], e o `Enquadramento` do servidor, que
/// recusa qualquer byte que não conheça.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TipoDeQuadro {
    /// Imagem que depende do quadro anterior.
    Comum = 0,
    /// Imagem que basta a si mesma. É por onde quem chega no meio entra.
    Chave = 1,
    /// Som do que está sendo mostrado, em Opus.
    ///
    /// **Dentro do mesmo fluxo da imagem, e não ao lado dele.** Dois fluxos
    /// chegariam em ordens diferentes e precisariam de carimbo e de fila de
    /// alinhamento; no mesmo fluxo, a ordem de chegada é a ordem de saída. E o
    /// orçamento de banda da tela já mede o fluxo inteiro — um segundo fluxo
    /// precisaria de um segundo teto e de uma segunda decisão sobre o que cede.
    Som = 2,
}

impl TipoDeQuadro {
    /// O byte que vai no fio.
    #[must_use]
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// O tipo que um byte nomeia, ou nada.
    ///
    /// `None` é um fluxo que não é o nosso, e quem lê o trata como fim — nunca
    /// como um quadro de tipo desconhecido a pular. Pular exigiria confiar no
    /// tamanho que veio junto, e o tamanho é a única coisa que um fluxo de lixo
    /// pode usar para pedir uma alocação absurda.
    #[must_use]
    pub const fn de_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Comum),
            1 => Some(Self::Chave),
            2 => Some(Self::Som),
            _ => None,
        }
    }

    /// É imagem que basta a si mesma?
    #[must_use]
    pub const fn e_chave(self) -> bool {
        matches!(self, Self::Chave)
    }
}

/// Escreve o cabeçalho de um quadro nos primeiros [`CABECALHO_DE_QUADRO_LEN`]
/// bytes.
fn escrever_cabecalho_de_quadro(tipo: TipoDeQuadro, tamanho: u32) -> [u8; CABECALHO_DE_QUADRO_LEN] {
    let mut bytes = [0_u8; CABECALHO_DE_QUADRO_LEN];
    // Big-endian, pelo motivo que `seele_proto::media` já dá: é o que todo
    // protocolo de mídia em tempo real escreve, então uma captura aberta no
    // Wireshark se lê do jeito que um engenheiro espera.
    let tamanho = tamanho.to_be_bytes();
    bytes[0] = tipo.byte();
    bytes[1] = tamanho[0];
    bytes[2] = tamanho[1];
    bytes[3] = tamanho[2];
    bytes[4] = tamanho[3];
    bytes
}

// ---------------------------------------------------------------------------
// Erros
// ---------------------------------------------------------------------------

/// Por que uma transmissão não abriu, não escreveu ou não leu.
///
/// Enumerado por `specs/02-protocolo.md`, e as duas metades de rede vêm como
/// texto de propósito: os erros do `quinn` são tipos do `quinn`, e devolvê-los
/// inteiros poria a versão de um crate de transporte na API pública deste — que
/// é a mesma razão pela qual [`crate::FlowControl`] copia quatro contadores em
/// vez de reexportar `quinn::ConnectionStats`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ErroDeTela {
    /// O cabeçalho de abertura não serve.
    #[error("screen header refused: {0}")]
    Cabecalho(#[from] ScreenError),
    /// O par anunciou um quadro maior do que este build carrega.
    #[error("peer announced a {len}-byte picture, over the {MAX_QUADRO_LEN}-byte limit")]
    QuadroGrandeDemais {
        /// O tamanho anunciado.
        len: usize,
    },
    /// O par anunciou um quadro vazio.
    ///
    /// Recusado com o mesmo fôlego que o grande demais, pelo motivo que
    /// `ScreenHeader::check` dá sobre um lado de zero: é muito mais vezes uma
    /// captura que falhou do que uma escolha, e não há quadro atrás dele de
    /// qualquer jeito.
    #[error("peer announced an empty picture")]
    QuadroVazio,
    /// A conexão, o fluxo ou a leitura acabaram.
    #[error("screen stream: {0}")]
    Fluxo(String),
}

// ---------------------------------------------------------------------------
// O lado de quem compartilha
// ---------------------------------------------------------------------------

/// O que aconteceu com um quadro entregue a [`Transmissao::enviar_quadro`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Envio {
    /// Saiu inteiro.
    Enviado,
    /// É quadro-chave, e a primeira fatia saiu. O resto sai nos próximos
    /// tiques, uma fatia por chamada. Ver [`FATIAS_DO_QUADRO_CHAVE`].
    Espalhando,
    /// Não saiu, e o motivo é dizível.
    Descartado(MotivoDeDescarte),
}

/// Por que um quadro não saiu.
///
/// **Descartar é a política, não a falha.** É a mesma decisão que
/// `specs/03-audio.md` já tomou para o áudio e que o §1 repete para a captura:
/// um quadro velho entregue tarde é pior que um quadro perdido, e uma fila de
/// quadros de 1080p come memória depressa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MotivoDeDescarte {
    /// O quadro não cabia no que resta do orçamento deste segundo.
    #[error("the picture did not fit the ceiling's budget")]
    AcimaDoTeto,
    /// Um quadro-chave ainda está saindo em fatias.
    ///
    /// Enquanto ele sai, nada mais pode ser escrito: o fluxo é uma sequência
    /// ordenada de bytes, e um quadro comum escrito no meio das fatias sairia
    /// **dentro** do quadro-chave. Descartar é a saída certa e barata — quem
    /// recebe não teria o que fazer com esses quadros de qualquer jeito, porque
    /// eles predizem justamente do quadro-chave que ainda não chegou.
    #[error("a key frame is still being spread over the stream")]
    QuadroChaveEmVoo,
    /// O quadro passa de [`MAX_QUADRO_LEN`].
    #[error("the picture is over the {MAX_QUADRO_LEN}-byte limit")]
    GrandeDemais,
    /// O quadro estava vazio.
    #[error("there was no picture")]
    Vazio,
}

/// O que falta escrever de um quadro-chave espalhado.
#[derive(Debug)]
struct ChaveEmVoo {
    bytes: Vec<u8>,
    escrito: usize,
    fatia: usize,
}

/// Uma transmissão de tela saindo desta máquina.
///
/// Um fluxo unidirecional por transmissão, aberto por quem compartilha, na
/// conexão QUIC que já existe (§3.1 e §3.6).
#[derive(Debug)]
pub struct Transmissao {
    fluxo: quinn::SendStream,
    cabecalho: ScreenHeader,
    balde: Balde,
    em_voo: Option<ChaveEmVoo>,
    /// Os pacotes de som esperando o fluxo ficar entre quadros.
    ///
    /// **Existe porque escrever no meio de um quadro-chave corrompe o fluxo.**
    /// Ver [`Self::enviar_som`].
    som_pendente: std::collections::VecDeque<Vec<u8>>,
    enviados: u64,
    descartados: u64,
    bytes_enviados: u64,
}

impl Transmissao {
    /// Abre o fluxo e escreve o cabeçalho de abertura.
    ///
    /// `teto_bps` é o que [`TetoDeVideo::teto`] devolveu. Uma transmissão só se
    /// abre com [`Teto::Bps`]: [`Teto::Parado`] não é um teto baixo, é a
    /// resposta de que não se transmite agora — e um valor que serve para os
    /// dois casos seria a interface ensinando a ignorar a diferença.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::Cabecalho`] se o cabeçalho não passa em
    /// `ScreenHeader::check`, ou [`ErroDeTela::Fluxo`] se a conexão não abre.
    pub async fn abrir(
        conexao: &quinn::Connection,
        cabecalho: ScreenHeader,
        teto_bps: u32,
        agora: Instant,
    ) -> Result<Self, ErroDeTela> {
        let mut abertura = [0_u8; SCREEN_HEADER_LEN];
        // Antes de abrir o fluxo, e não depois: uma resolução que a prova não
        // cobre não vale um fluxo aberto que só será fechado.
        cabecalho.encode(&mut abertura)?;

        let mut fluxo = conexao
            .open_uni()
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        // Antes do primeiro byte, para que nem a abertura passe na frente do
        // controle. `set_priority` só falha em fluxo já fechado, que aqui
        // acabou de nascer — e mesmo assim não vale derrubar a transmissão por
        // uma prioridade não aplicada.
        let _ = fluxo.set_priority(PRIORIDADE_DA_TELA);
        // O tipo do fluxo antes do cabeçalho, pelo mesmo motivo do anexo: quem
        // recebe precisa saber o que está chegando **antes** de tentar ler o
        // que chegou. Ver o §5.2 da spec.
        if let Err(erro) = fluxo
            .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
            .await
        {
            return Err(ErroDeTela::Fluxo(erro.to_string()));
        }
        fluxo
            .write_all(&abertura)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;

        Ok(Self {
            som_pendente: std::collections::VecDeque::new(),
            fluxo,
            cabecalho,
            balde: Balde::novo(teto_bps, agora),
            em_voo: None,
            enviados: 0,
            descartados: 0,
            bytes_enviados: 0,
        })
    }

    /// O cabeçalho com que esta transmissão abriu.
    #[must_use]
    pub const fn cabecalho(&self) -> &ScreenHeader {
        &self.cabecalho
    }

    /// Troca o teto, mantendo o que já foi gasto neste segundo.
    ///
    /// Chamado toda vez que a faixa do sinal muda. **Não repõe o balde**: um
    /// teto novo que devolvesse fichas faria uma queda de faixa liberar uma
    /// rajada, que é o oposto exato do que a queda de faixa quer dizer.
    pub fn ajustar_teto(&mut self, teto_bps: u32, agora: Instant) {
        self.balde.repor(agora);
        let por_segundo = f64::from(teto_bps) / 8.0;
        self.balde.capacidade = por_segundo;
        self.balde.por_segundo = por_segundo;
        self.balde.fichas = self.balde.fichas.min(por_segundo);
    }

    /// Entrega um quadro codificado ao fluxo, ou diz por que não.
    ///
    /// Um quadro comum sai inteiro. Um quadro-chave sai em
    /// [`FATIAS_DO_QUADRO_CHAVE`] fatias, uma por chamada, e é aí que o §3.3
    /// acontece: o cabeçalho do quadro anuncia o tamanho **inteiro** e as
    /// fatias vão preenchendo, porque um fluxo QUIC é uma sequência ordenada de
    /// bytes e quem lê só termina quando a última fatia chega. Não há bandeira
    /// de continuação nem remontagem do outro lado — espalhar é uma decisão de
    /// **quando escrever**, e não um formato.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::Fluxo`] quando o fluxo morreu. Um quadro recusado não é
    /// erro: volta como [`Envio::Descartado`], porque descartar é a política.
    /// Manda um pacote de som junto com a imagem.
    ///
    /// # Ele espera o quadro-chave acabar, e **é obrigatório**
    ///
    /// Um quadro-chave sai em [`FATIAS_DO_QUADRO_CHAVE`] fatias, uma por tique.
    /// O cabeçalho anuncia o tamanho **inteiro** e as fatias vão preenchendo:
    /// não há bandeira de continuação, porque um fluxo QUIC é uma sequência
    /// ordenada de bytes e quem lê só termina quando a última fatia chega.
    ///
    /// Escrever qualquer outra coisa no meio disso corrompe o fluxo. O receptor
    /// está contando bytes do quadro-chave, e um cabeçalho de som no meio vira
    /// payload de imagem: o quadro sai errado, o enquadramento perde o passo, e
    /// **tudo depois dele é lixo**.
    ///
    /// A primeira versão não esperava, de propósito — para não segurar o som
    /// por quatro tiques, que são 80 ms de silêncio a cada quadro-chave. O
    /// resultado em campo foi a transmissão inteira parar no primeiro quadro:
    /// «exibe apenas 1 frame e o Windows não consegue ver», sem som nenhum,
    /// porque o som tinha sido lido como imagem. Trocar 80 ms de silêncio por
    /// uma transmissão que não anda é um negócio ruim.
    ///
    /// Então ele enfileira. A fila tem teto e joga fora o **mais velho** —
    /// quem está atrasado meio segundo já perdeu a sincronia com a imagem, e
    /// insistir nele afasta o som cada vez mais.
    ///
    /// # O que continua valendo
    ///
    /// Ele não passa pelo balde do teto de banda. O som custa 32 kbps contra os
    /// 1200 kbps do vídeo — menos de 3% do orçamento — e é a metade da
    /// transmissão que continua útil quando a imagem engasga: um jogo a 8
    /// quadros **com som** é acompanhável; a 30 quadros mudo, não.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::Fluxo`] quando o fluxo já foi embora.
    pub async fn enviar_som(&mut self, bytes: &[u8]) -> Result<Envio, ErroDeTela> {
        /// Meio segundo a 20 ms por pacote.
        const TETO_DA_FILA: usize = 25;

        if bytes.is_empty() {
            return Ok(Envio::Descartado(MotivoDeDescarte::Vazio));
        }
        if bytes.len() > MAX_QUADRO_LEN {
            return Ok(Envio::Descartado(MotivoDeDescarte::GrandeDemais));
        }

        while self.som_pendente.len() >= TETO_DA_FILA {
            self.som_pendente.pop_front();
        }
        self.som_pendente.push_back(bytes.to_vec());
        self.escoar_som().await
    }

    /// Anda com o quadro-chave em voo sem que um quadro novo precise chegar.
    ///
    /// [`Self::enviar_quadro`] escreve uma fatia por chamada, e quem a chama só
    /// chama quando a captura entrega imagem nova. Um tique sem imagem —
    /// `SemQuadro`, a tela parada — e um tique que o controle de taxa pulou —
    /// `PuladoPeloTeto` — não a chamavam, e o quadro-chave ficava pela metade
    /// no fio: quem assiste não fecha um quadro para decodificar, e quem
    /// compartilha também não vê nada, porque enquanto há chave em voo todo
    /// quadro que chega é descartado, o do espelho local inclusive.
    ///
    /// Não conta descarte. Nada foi descartado aqui — não havia quadro para
    /// descartar, e é essa a diferença entre este caminho e o de cima.
    pub async fn escoar_chave(&mut self) -> Result<(), ErroDeTela> {
        let Some(mut voando) = self.em_voo.take() else {
            return Ok(());
        };
        if self.escrever_fatia(&mut voando).await? {
            // Fechou. O fluxo está entre quadros pela primeira vez desde que
            // esta chave começou, e é a porta por onde o som que esperou sai.
            self.escoar_som().await?;
        } else {
            self.em_voo = Some(voando);
        }
        Ok(())
    }

    /// Escreve o som enfileirado, se o fluxo estiver entre quadros.
    ///
    /// Chamada por [`Self::enviar_som`] e por quem termina um quadro-chave: as
    /// duas portas por onde o fluxo pode ficar livre.
    async fn escoar_som(&mut self) -> Result<Envio, ErroDeTela> {
        if self.em_voo.is_some() {
            return Ok(Envio::Espalhando);
        }
        let mut foi = false;
        while let Some(pacote) = self.som_pendente.pop_front() {
            // `u32` cabe: `MAX_QUADRO_LEN` é 512 KiB e já foi conferido.
            let tamanho = u32::try_from(pacote.len()).unwrap_or(u32::MAX);
            self.escrever(&escrever_cabecalho_de_quadro(TipoDeQuadro::Som, tamanho))
                .await?;
            self.escrever(&pacote).await?;
            self.bytes_enviados += (CABECALHO_DE_QUADRO_LEN + pacote.len()) as u64;
            foi = true;
        }
        Ok(if foi {
            Envio::Enviado
        } else {
            Envio::Espalhando
        })
    }

    /// Entrega um quadro codificado ao fluxo, ou diz por que não.
    ///
    /// Um quadro comum sai inteiro. Um quadro-chave sai em
    /// [`FATIAS_DO_QUADRO_CHAVE`] fatias, uma por chamada, e é aí que o §3.3
    /// acontece: o cabeçalho do quadro anuncia o tamanho **inteiro** e as
    /// fatias vão preenchendo, porque um fluxo QUIC é uma sequência ordenada de
    /// bytes e quem lê só termina quando a última fatia chega. Não há bandeira
    /// de continuação nem remontagem do outro lado — espalhar é uma decisão de
    /// **quando escrever**, e não um formato.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::Fluxo`] quando o fluxo morreu. Um quadro recusado não é
    /// erro: volta como [`Envio::Descartado`], porque descartar é a política.
    pub async fn enviar_quadro(
        &mut self,
        bytes: &[u8],
        chave: bool,
        agora: Instant,
    ) -> Result<Envio, ErroDeTela> {
        if let Some(mut voando) = self.em_voo.take() {
            let acabou = self.escrever_fatia(&mut voando).await?;
            if !acabou {
                self.em_voo = Some(voando);
            } else {
                // O quadro-chave acabou de fechar, e o fluxo está entre quadros
                // pela primeira vez desde que ele começou: é aqui que o som que
                // esperou pode sair, e antes do próximo quadro entrar na frente.
                self.escoar_som().await?;
            }
            self.descartados += 1;
            return Ok(Envio::Descartado(MotivoDeDescarte::QuadroChaveEmVoo));
        }

        if bytes.is_empty() {
            self.descartados += 1;
            return Ok(Envio::Descartado(MotivoDeDescarte::Vazio));
        }
        if bytes.len() > MAX_QUADRO_LEN {
            self.descartados += 1;
            return Ok(Envio::Descartado(MotivoDeDescarte::GrandeDemais));
        }

        let total = CABECALHO_DE_QUADRO_LEN.saturating_add(bytes.len());
        if !self.balde.gastar(total, agora) {
            self.descartados += 1;
            return Ok(Envio::Descartado(MotivoDeDescarte::AcimaDoTeto));
        }

        // `u32` cabe: `MAX_QUADRO_LEN` é 512 KiB e já foi conferido acima.
        let tamanho = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let tipo = if chave {
            TipoDeQuadro::Chave
        } else {
            TipoDeQuadro::Comum
        };
        self.escrever(&escrever_cabecalho_de_quadro(tipo, tamanho))
            .await?;
        self.enviados += 1;
        self.bytes_enviados += total as u64;

        if !chave {
            self.escrever(bytes).await?;
            return Ok(Envio::Enviado);
        }

        let mut voando = ChaveEmVoo {
            bytes: bytes.to_vec(),
            escrito: 0,
            fatia: 0,
        };
        let acabou = self.escrever_fatia(&mut voando).await?;
        if acabou {
            // Cabe numa fatia só: um quadro-chave de 540p a 8 quadros pode ser
            // menor que quatro pedaços úteis. Espalhar quatro bytes em quatro
            // escritas não protege voz nenhuma.
            return Ok(Envio::Enviado);
        }
        self.em_voo = Some(voando);
        Ok(Envio::Espalhando)
    }

    /// Escreve a próxima fatia. Devolve `true` quando não sobra nada.
    async fn escrever_fatia(&mut self, voando: &mut ChaveEmVoo) -> Result<bool, ErroDeTela> {
        let total = voando.bytes.len();
        voando.fatia += 1;
        // A última fatia leva o resto, para que arredondamento não deixe bytes
        // órfãos: quem lê espera exatamente `tamanho` bytes e ficaria pendurado
        // para sempre por causa de uma divisão.
        let fim = if voando.fatia >= FATIAS_DO_QUADRO_CHAVE {
            total
        } else {
            (total.div_ceil(FATIAS_DO_QUADRO_CHAVE) * voando.fatia).min(total)
        };
        let inicio = voando.escrito;
        let pedaco = voando.bytes.get(inicio..fim).unwrap_or_default().to_vec();
        self.escrever(&pedaco).await?;
        voando.escrito = fim;
        Ok(fim >= total)
    }

    async fn escrever(&mut self, bytes: &[u8]) -> Result<(), ErroDeTela> {
        self.fluxo
            .write_all(bytes)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))
    }

    /// Quantos quadros saíram, quantos foram descartados, quantos bytes foram.
    #[must_use]
    pub const fn contagem(&self) -> (u64, u64, u64) {
        (self.enviados, self.descartados, self.bytes_enviados)
    }

    /// Fecha o fluxo, dizendo a quem recebe que a transmissão acabou.
    ///
    /// O fim do fluxo é a segunda maneira de dizer «parei», e a de controle
    /// (`ClientMessage::StopScreenShare`) é a primeira. As duas existem porque
    /// uma delas — esta — também acontece quando a máquina simplesmente some, e
    /// o §3.6 quer que a sala consiga distinguir «ela parou de compartilhar» de
    /// «o enlace dela caiu».
    pub fn encerrar(mut self) {
        let _ = self.fluxo.finish();
    }
}

// ---------------------------------------------------------------------------
// O lado de quem assiste
// ---------------------------------------------------------------------------

/// Um quadro codificado que chegou inteiro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroRecebido {
    /// O que veio dentro: imagem comum, imagem-chave, ou som.
    ///
    /// Era `chave: bool`, e virou um tipo de três valores quando o som passou a
    /// viajar no mesmo fluxo. Quem só precisava daquela pergunta usa
    /// [`Self::chave`].
    pub tipo: TipoDeQuadro,
    /// Os bytes, em Annex-B, como o encoder do outro lado os produziu — ou um
    /// pacote Opus, quando o tipo é [`TipoDeQuadro::Som`].
    pub bytes: Vec<u8>,
}

impl QuadroRecebido {
    /// É imagem que basta a si mesma?
    #[must_use]
    pub const fn chave(&self) -> bool {
        self.tipo.e_chave()
    }
}

/// Uma transmissão de tela chegando nesta máquina.
#[derive(Debug)]
pub struct Recepcao {
    fluxo: quinn::RecvStream,
    cabecalho: ScreenHeader,
}

impl Recepcao {
    /// Aceita o próximo fluxo unidirecional e lê o cabeçalho de abertura.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::Cabecalho`] para um cabeçalho malformado — versão
    /// estranha, fonte ou codec desconhecidos, resolução fora do teto — e
    /// [`ErroDeTela::Fluxo`] quando a conexão acaba.
    pub async fn aceitar(conexao: &quinn::Connection) -> Result<Self, ErroDeTela> {
        let fluxo = conexao
            .accept_uni()
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        Self::do_fluxo(fluxo).await
    }

    /// A mesma leitura, sobre um fluxo que quem chama já aceitou.
    ///
    /// Existe porque uma conexão tem um `accept_uni` só e mais de um uso para
    /// fluxo unidirecional: quem multiplexa aceita e decide, e esta metade é a
    /// que sabe ler tela.
    ///
    /// # Errors
    ///
    /// As mesmas de [`Self::aceitar`].
    pub async fn do_fluxo(mut fluxo: quinn::RecvStream) -> Result<Self, ErroDeTela> {
        // O tipo do fluxo primeiro, e conferido em vez de pulado: um byte lido
        // e jogado fora aceitaria um anexo como se fosse tela e leria o
        // cabeçalho errado sem reclamar. Ver o §5.2 da spec.
        let mut tipo = [0_u8; seele_proto::stream::STREAM_TYPE_LEN];
        fluxo
            .read_exact(&mut tipo)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        match seele_proto::stream::StreamType::decode(tipo[0]) {
            Ok(seele_proto::stream::StreamType::Screen) => {}
            Ok(outro) => {
                return Err(ErroDeTela::Fluxo(format!(
                    "este fluxo é {outro:?} e não uma transmissão de tela"
                )));
            }
            Err(erro) => return Err(ErroDeTela::Fluxo(erro.to_string())),
        }

        Self::do_fluxo_ja_tipado(fluxo).await
    }

    /// A mesma leitura, sobre um fluxo cujo byte de tipo já foi lido.
    ///
    /// É esta que a produção usa: quem aceita os fluxos de entrada é o roteador
    /// de `Client::connect`, e é lá que o byte é lido — ele precisa lê-lo para
    /// saber para qual fila mandar o fluxo, e lê-lo duas vezes comeria o
    /// primeiro byte do cabeçalho.
    ///
    /// # Errors
    ///
    /// As mesmas de [`Self::aceitar`], menos as do byte de tipo.
    pub async fn do_fluxo_ja_tipado(mut fluxo: quinn::RecvStream) -> Result<Self, ErroDeTela> {
        let mut abertura = [0_u8; SCREEN_HEADER_LEN];
        fluxo
            .read_exact(&mut abertura)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        let (cabecalho, _) = ScreenHeader::decode(&abertura)?;
        Ok(Self { fluxo, cabecalho })
    }

    /// O cabeçalho com que esta transmissão abriu.
    #[must_use]
    pub const fn cabecalho(&self) -> &ScreenHeader {
        &self.cabecalho
    }

    /// Lê o próximo quadro, ou `None` quando o outro lado encerrou.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::QuadroGrandeDemais`] ou [`ErroDeTela::QuadroVazio`] para
    /// um tamanho que este build não carrega — conferido **antes** de alocar —,
    /// e [`ErroDeTela::Fluxo`] para um fluxo cortado no meio de um quadro.
    pub async fn proximo_quadro(&mut self) -> Result<Option<QuadroRecebido>, ErroDeTela> {
        let mut cabecalho = [0_u8; CABECALHO_DE_QUADRO_LEN];
        match self.fluxo.read_exact(&mut cabecalho).await {
            Ok(()) => {}
            // O fim limpo do fluxo: quem compartilha parou. Não é erro, e
            // tratá-lo como erro faria toda transmissão terminar com uma
            // mensagem de falha na tela de quem assistia até o fim.
            Err(quinn::ReadExactError::FinishedEarly(0)) => return Ok(None),
            Err(erro) => return Err(ErroDeTela::Fluxo(erro.to_string())),
        }

        // Um byte que não nomeia tipo nenhum é um fluxo que não é o nosso, e a
        // resposta é parar de ler — nunca pular pelo tamanho que veio junto,
        // que é o único número que um fluxo de lixo controla.
        let Some(tipo) = TipoDeQuadro::de_byte(cabecalho.first().copied().unwrap_or_default())
        else {
            return Err(ErroDeTela::QuadroVazio);
        };
        let tamanho = cabecalho
            .get(1..CABECALHO_DE_QUADRO_LEN)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map_or(0, u32::from_be_bytes) as usize;

        // Antes de alocar. `specs/08-seguranca.md`.
        if tamanho == 0 {
            return Err(ErroDeTela::QuadroVazio);
        }
        if tamanho > MAX_QUADRO_LEN {
            return Err(ErroDeTela::QuadroGrandeDemais { len: tamanho });
        }

        let mut bytes = vec![0_u8; tamanho];
        self.fluxo
            .read_exact(&mut bytes)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        Ok(Some(QuadroRecebido { tipo, bytes }))
    }
}

/// Quanto dura um intervalo de quadro a esta cadência.
///
/// Aqui e não no `seele-video` porque quem espalha o quadro-chave é o
/// transporte, e o intervalo é a unidade em que ele espalha.
#[must_use]
pub fn intervalo_de_quadro(quadros_por_segundo: u32) -> Duration {
    if quadros_por_segundo == 0 {
        return Duration::ZERO;
    }
    Duration::from_secs(1) / quadros_por_segundo
}

#[cfg(test)]
pub(crate) mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use seele_proto::ids::ScreenId;
    use seele_proto::screen::{ScreenCodec, ScreenSource};

    use super::*;

    fn cabecalho() -> ScreenHeader {
        ScreenHeader {
            version: seele_proto::version::PROTOCOL_VERSION,
            screen: ScreenId(0x00C0_FFEE),
            source: ScreenSource::Monitor,
            codec: ScreenCodec::H264Baseline,
            width: 1280,
            height: 720,
        }
    }

    // -----------------------------------------------------------------------
    // O teto de banda
    // -----------------------------------------------------------------------

    /// **O critério de aceite do ciclo inteiro**, escrito como teste e não como
    /// comentário.
    ///
    /// §3.2, regra 3: *«a voz nunca cede à tela. Quando o sinal cai de faixa,
    /// quem baixa é o vídeo; se continuar caindo, quem para é o vídeo. Uma
    /// conversa com a tela travando é o produto funcionando; uma conversa
    /// picotando porque alguém abriu a tela é o produto quebrado.»*
    ///
    /// Três propriedades, e as três têm de valer ao mesmo tempo em todo caminho
    /// e em toda faixa:
    ///
    /// 1. o teto do vídeo **nunca** passa de [`FRACAO_DO_CAMINHO`] do caminho —
    ///    é o único ponto em que `spikes/tela-no-transporte` viu a voz voltar
    ///    aos 23,1 ms de p50 e 0% de perda, contra 225,7 ms com o vídeo solto;
    /// 2. a reserva da voz **não depende da faixa**. Quando o sinal piora, o
    ///    que encolhe é o vídeo, e o que a voz tem reservado é o mesmo número;
    /// 3. piorar a faixa nunca dá mais ao vídeo, e a faixa crítica **para** o
    ///    vídeo com motivo enumerado em vez de deixá-lo caindo para sempre.
    ///
    /// E, desde 22/08/2026, uma quarta, que é a perna que o §5.1 acrescentou
    /// quando decidiu que **o servidor encaminha**:
    ///
    /// 4. com N espectadores crescendo, o teto **cai** e a reserva da voz
    ///    **nunca** encolhe. As N cópias saem da subida de quem hospeda, e o
    ///    que elas somam nunca passa dos mesmos 60% de lá — porque a voz de
    ///    todo mundo passa por aquela máquina também. Sem esta propriedade o
    ///    produto media uma perna e estourava a outra, que é o defeito que o
    ///    §5.1 chama de o mais caro daquela seção.
    ///
    /// Se este teste ficar vermelho, o recurso está pior que não existir: é a
    /// tela tornando a conversa impossível, que é exatamente o que o spike
    /// mediu e o que este ciclo existe para não repetir.
    #[test]
    fn a_voz_nunca_cede_a_tela() {
        for caminho in [
            PISO_DE_BANDA_BPS * 4,
            1_000_000,
            CAMINHO_DA_PROVA_BPS,
            10_000_000,
            // Fibra doméstica, onde um `u32` estourando na multiplicação por 60
            // daria um teto minúsculo e o defeito só apareceria na casa boa.
            900_000_000,
        ] {
            let teto = TetoDeVideo::com_caminho(caminho);
            let reserva = teto.reserva_da_voz();

            // 0 — a reserva é de verdade, e vale os 40% que o spike mediu.
            //
            // O 60 vai escrito à mão, e não como [`FRACAO_DO_CAMINHO`], de
            // propósito: com a constante, este teste concordaria com qualquer
            // número que alguém pusesse nela — inclusive 100, que zera a
            // reserva e faz as três propriedades abaixo passarem enquanto a voz
            // volta aos 225,7 ms que este ciclo existe para não repetir.
            let reserva_minima = caminho - ((u64::from(caminho) * 60 / 100) as u32);
            assert!(
                reserva >= reserva_minima,
                "num caminho de {caminho} bps sobraram só {reserva} para a voz"
            );

            let mut anterior = u32::MAX;
            for faixa in [
                SignalBand::Nominal,
                SignalBand::Degraded,
                SignalBand::Critical,
            ] {
                let agora = teto.teto(faixa);

                // 1 — o vídeo nunca passa da fração medida, então a voz sempre
                // tem os outros 40% do caminho para chegar em 23 ms.
                assert!(
                    u64::from(agora.bps()) + u64::from(reserva) <= u64::from(caminho),
                    "em {faixa:?} sobre {caminho} bps o vídeo levou {} e a voz tinha {reserva}",
                    agora.bps()
                );

                // 2 — a reserva da voz é a mesma nas três faixas. Quem cede é o
                // vídeo, sempre, e é isto que a frase quer dizer em aritmética.
                assert_eq!(
                    teto.reserva_da_voz(),
                    reserva,
                    "a reserva da voz mudou de tamanho em {faixa:?}"
                );

                // 3 — cair de faixa nunca dá mais ao vídeo.
                assert!(
                    agora.bps() <= anterior,
                    "em {faixa:?} o vídeo ganhou banda ao piorar o sinal"
                );
                anterior = agora.bps();
            }

            // E o fim da escada é parar, com nome — não é um teto muito baixo.
            assert_eq!(
                teto.teto(SignalBand::Critical),
                Teto::Parado(MotivoDeParada::SinalCritico),
                "sinal crítico tinha de parar o vídeo em {caminho} bps"
            );

            // ---------------------------------------------------------------
            // 4 — a perna que o §5.1 acrescentou. O servidor encaminha, e quem
            // sobe N cópias é quem hospeda; aqui as duas casas têm o mesmo
            // caminho, que é o caso em que a única coisa que se mexe é o N.
            // ---------------------------------------------------------------
            let mut anterior_com_n = u32::MAX;
            for espectadores in [0, 1, 2, 4, 10, 1_000] {
                let com_n = TetoDeVideo::com_caminho(caminho)
                    .com_caminho_de_quem_hospeda(caminho)
                    .com_espectadores(espectadores);

                // 4a — a reserva da voz não sabe quantas pessoas entraram. É a
                // mesma frase de sempre, agora contra a coisa que mudou: uma
                // sala que cresce aperta o vídeo, nunca a voz.
                assert_eq!(
                    com_n.reserva_da_voz(),
                    reserva,
                    "a reserva da voz encolheu com {espectadores} espectadores em {caminho} bps"
                );

                for faixa in [SignalBand::Nominal, SignalBand::Degraded] {
                    let agora = com_n.teto(faixa);

                    // 4b — o que o servidor sobe, que são N cópias do mesmo
                    // teto, nunca passa dos 60% do caminho de quem hospeda. Os
                    // outros 40% são a voz de todo mundo que passa por lá, e
                    // esta é a única linha que prova que a divisão por N está
                    // sendo feita antes do min e não depois.
                    //
                    // O 60 vai escrito à mão pelo mesmo motivo da propriedade
                    // 0: com a constante, este teste concordaria com 100.
                    let subida = u64::from(agora.bps()) * u64::from(espectadores);
                    assert!(
                        subida <= u64::from(caminho) * 60 / 100,
                        "com {espectadores} espectadores em {faixa:?} o servidor subiria \
                         {subida} bps de tela num caminho de {caminho}"
                    );
                }

                // 4c — mais gente nunca dá mais banda ao vídeo. É o outro lado
                // de 4b: se o teto subisse com N, a conta acima ainda fecharia
                // por acaso em algum caminho largo.
                let agora = com_n.teto(SignalBand::Nominal).bps();
                assert!(
                    agora <= anterior_com_n,
                    "o teto subiu ao entrar mais gente: {anterior_com_n} → {agora} bps \
                     com {espectadores} espectadores em {caminho} bps"
                );
                anterior_com_n = agora;
            }

            // 4d — ninguém assistindo não é divisão por zero e também não é
            // teto infinito. Zero e um dão a mesma conta, que é o que faz a
            // entrada da primeira pessoa não mexer em nada.
            let vazio = TetoDeVideo::com_caminho(caminho)
                .com_caminho_de_quem_hospeda(caminho)
                .com_espectadores(0);
            let uma_pessoa = vazio.com_espectadores(1);
            assert_eq!(
                vazio.teto(SignalBand::Nominal),
                uma_pessoa.teto(SignalBand::Nominal),
                "sala vazia e sala de uma pessoa deram tetos diferentes em {caminho} bps"
            );
            assert_eq!(
                vazio.espectadores(),
                0,
                "o número que a tela escreve tem de ser o de verdade, não o da conta"
            );
        }
    }

    #[test]
    fn o_teto_e_uma_fracao_do_caminho_e_nao_um_numero_fixo() {
        // §3.2, regra 1: «o vídeo tem teto, e o teto é uma fração do caminho
        // medido, não um valor fixo de configuração». Um caminho duas vezes
        // maior dá um teto duas vezes maior; um teto fixo daria o mesmo nos
        // dois e seria estreito num e desperdiçado no outro.
        let estreito = TetoDeVideo::com_caminho(1_000_000);
        let largo = TetoDeVideo::com_caminho(2_000_000);
        assert_eq!(estreito.teto(SignalBand::Nominal), Teto::Bps(600_000));
        assert_eq!(largo.teto(SignalBand::Nominal), Teto::Bps(1_200_000));

        // E o padrão é o cano da prova, que dá exatamente os 1200 kbps sob os
        // quais as duas provas rodaram.
        assert_eq!(
            TetoDeVideo::novo().teto(SignalBand::Nominal),
            Teto::Bps(1_200_000)
        );
    }

    #[test]
    fn a_escolha_da_pessoa_e_teto_e_nunca_piso() {
        // §5, a regra que não se negocia. Escolher mais que o caminho aguenta
        // não levanta o teto; escolher menos abaixa.
        let caminho = TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS);

        let pedindo_demais = caminho.com_escolha(Some(50_000_000));
        assert_eq!(
            pedindo_demais.teto(SignalBand::Nominal),
            Teto::Bps(1_200_000),
            "a escolha virou piso e levantou o teto do caminho"
        );

        let pedindo_pouco = caminho.com_escolha(Some(500_000));
        assert_eq!(pedindo_pouco.teto(SignalBand::Nominal), Teto::Bps(500_000));

        // E continua sendo teto depois de o sinal cair: a faixa degradada corta
        // o que o caminho dá, e a escolha continua por cima do resultado.
        assert_eq!(
            caminho
                .com_escolha(Some(400_000))
                .teto(SignalBand::Degraded),
            Teto::Bps(400_000)
        );
    }

    #[test]
    fn abaixo_do_piso_o_video_para_com_nome_em_vez_de_degradar_para_sempre() {
        // §2: «se o encoder não sustenta nem o piso, o compartilhamento para,
        // com motivo enumerado. Degradar para sempre é como um instrumento
        // falso: consultado justamente quando algo deu errado.»
        let apertado = TetoDeVideo::com_caminho(500_000);
        assert_eq!(apertado.teto(SignalBand::Nominal), Teto::Bps(300_000));
        // Metade de 300 kbps são 150, abaixo do piso de 200.
        assert_eq!(
            apertado.teto(SignalBand::Degraded),
            Teto::Parado(MotivoDeParada::AbaixoDoPiso)
        );

        // E pela escolha da pessoa também: pedir menos que o piso é pedir para
        // não transmitir, e o produto diz isso em vez de transmitir um borrão.
        assert_eq!(
            TetoDeVideo::novo()
                .com_escolha(Some(PISO_DE_BANDA_BPS - 1))
                .teto(SignalBand::Nominal),
            Teto::Parado(MotivoDeParada::AbaixoDoPiso)
        );
    }

    #[test]
    fn a_resolucao_acompanha_o_teto_e_nao_a_contagem_de_gente() {
        // §5.1: «o gatilho é o teto, que já tem N dentro dele». O pedido era
        // «mais de 4 pessoas vai para 720p», e a razão de recusá-lo é que
        // resolução não controla tráfego: dez pessoas numa fibra cabem em
        // 1080p, quatro numa subida ruim não cabem em 720p.
        //
        // Dez pessoas numa fibra de 200 Mbps. **Eram 100**, e o número subiu
        // com a régua e não com a intenção: 100 Mbps ÷ 10 davam 6 Mbps de teto,
        // que compravam 1080p quando o limiar era 1,5 e não compram agora que
        // ele é 6,24. O que o teste diz continua sendo o mesmo — o degrau segue
        // o teto e não a contagem de gente.
        let fibra = TetoDeVideo::com_caminho(200_000_000)
            .com_caminho_de_quem_hospeda(200_000_000)
            .com_espectadores(10);
        assert_eq!(
            fibra.teto(SignalBand::Nominal).resolucao_estimada(),
            Some(Resolucao::P1080),
            "dez pessoas numa fibra continuam cabendo em 1080p"
        );

        // Quatro numa subida doméstica de 2 Mbps, que é o cano das duas provas:
        let casa = TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS)
            .com_caminho_de_quem_hospeda(CAMINHO_DA_PROVA_BPS)
            .com_espectadores(4);
        // 1200 kbps ÷ 4 são 300, que não compram nem 720p.
        assert_eq!(casa.teto(SignalBand::Nominal), Teto::Bps(300_000));
        assert_eq!(
            casa.teto(SignalBand::Nominal).resolucao_estimada(),
            Some(Resolucao::P540),
            "quatro pessoas numa casa não cabem em 720p, e é o teto que diz isso"
        );

        // E o degrau anda com o teto, não com o número de pessoas: a mesma sala
        // de quatro numa subida dez vezes maior sobe de degrau sozinha.
        //
        // **As duas pernas juntas**, e não só a de quem hospeda: o `min` do
        // §5.1 tem três braços, e subir um só deixa o teto preso no outro — foi
        // o que aconteceu quando este teste subia apenas `com_caminho_de_quem_hospeda`
        // e a perna desta máquina continuava nos 2 Mbps da prova.
        let casa_boa = TetoDeVideo::com_caminho(20_000_000)
            .com_caminho_de_quem_hospeda(20_000_000)
            .com_espectadores(4);
        assert_eq!(
            casa_boa.teto(SignalBand::Nominal).resolucao_estimada(),
            Some(Resolucao::P720),
            "a mesma sala de quatro, com mais subida, tinha de subir de degrau"
        );
    }

    #[test]
    fn os_degraus_de_resolucao_sao_o_que_a_unica_medida_sustenta() {
        // A tabela do §5.1, medida num teto só — 1200 kbps. Os limiares são
        // estimativa, e o nome das constantes diz isso; o que este teste prende
        // é o que a medida de fato sustenta.

        // No teto que foi medido, 1080p joga fora 16,2% do que captura. Então
        // 1200 kbps **não** compra 1080p — e, desde que a régua passou a ser
        // bits por pixel, não compra 720p também: 720p a 1200 kbps e 30 quadros
        // são 0,043 bits por pixel, menos da metade do que texto pede. O degrau
        // que 1200 kbps compra é 540p, e comprá-lo nítido é melhor que comprar
        // 720p blocado.
        assert_eq!(resolucao_estimada_para(1_200_000), Resolucao::P540);

        // Nos limiares, e um bit abaixo de cada um.
        assert_eq!(
            resolucao_estimada_para(TETO_ESTIMADO_PARA_1080P_BPS),
            Resolucao::P1080
        );
        assert_eq!(
            resolucao_estimada_para(TETO_ESTIMADO_PARA_1080P_BPS - 1),
            Resolucao::P720
        );
        assert_eq!(
            resolucao_estimada_para(TETO_ESTIMADO_PARA_720P_BPS),
            Resolucao::P720
        );
        assert_eq!(
            resolucao_estimada_para(TETO_ESTIMADO_PARA_720P_BPS - 1),
            Resolucao::P540
        );

        // 540p é o piso da lista, e não há um degrau abaixo dele: quem para é o
        // piso de banda, com motivo, e não uma quarta resolução.
        assert_eq!(resolucao_estimada_para(PISO_DE_BANDA_BPS), Resolucao::P540);
        assert_eq!(resolucao_estimada_para(0), Resolucao::P540);

        // Nunca decresce: um teto maior nunca dá uma resolução menor.
        let mut anterior = Resolucao::P540;
        for teto in (0..3_000_000).step_by(50_000) {
            let agora = resolucao_estimada_para(teto);
            assert!(
                agora.largura() * agora.altura() >= anterior.largura() * anterior.altura(),
                "o degrau caiu ao subir o teto para {teto} bps"
            );
            anterior = agora;
        }

        // E parado não é 540p: é resolução nenhuma.
        assert_eq!(
            Teto::Parado(MotivoDeParada::SinalCritico).resolucao_estimada(),
            None
        );
    }

    #[test]
    fn a_escolha_de_resolucao_continua_teto_e_nunca_piso() {
        // §5, a mesma regra da banda. O degrau do teto e a escolha da pessoa
        // são os dois teto; quem manda é o menor.
        let fibra = resolucao_estimada_para(10_000_000);
        assert_eq!(fibra, Resolucao::P1080);
        assert_eq!(
            menor_resolucao(fibra, Resolucao::P540),
            Resolucao::P540,
            "quem escolheu 540p numa fibra continua em 540p"
        );

        let apertado = resolucao_estimada_para(500_000);
        assert_eq!(
            menor_resolucao(apertado, Resolucao::P1080),
            Resolucao::P540,
            "escolher 1080p não levanta o degrau que o orçamento comprou"
        );

        // E é simétrica: qual dos dois vem primeiro não muda a resposta.
        for a in Resolucao::TODAS {
            for b in Resolucao::TODAS {
                assert_eq!(menor_resolucao(a, b), menor_resolucao(b, a));
            }
        }
    }

    #[test]
    fn a_tela_pode_dizer_qual_perna_apertou() {
        // §5.1: «quando aperta, a tela diz que apertou e por quê». É a
        // diferença entre `720p · 6 pessoas assistindo` e `720p · sua conexão`.
        let sala_cheia = TetoDeVideo::com_caminho(50_000_000)
            .com_caminho_de_quem_hospeda(CAMINHO_DA_PROVA_BPS)
            .com_espectadores(4);
        assert_eq!(
            sala_cheia.perna_que_aperta(SignalBand::Nominal),
            PernaQueAperta::QuemHospeda
        );

        let subida_ruim = TetoDeVideo::com_caminho(600_000)
            .com_caminho_de_quem_hospeda(50_000_000)
            .com_espectadores(4);
        assert_eq!(
            subida_ruim.perna_que_aperta(SignalBand::Nominal),
            PernaQueAperta::QuemCompartilha
        );

        // A escolha da pessoa só «aperta» quando ela é a menor das três — pedir
        // mais do que o caminho dá não é a escolha mandando, é a rede.
        let escolheu_pouco = TetoDeVideo::novo().com_escolha(Some(400_000));
        assert_eq!(
            escolheu_pouco.perna_que_aperta(SignalBand::Nominal),
            PernaQueAperta::Escolha
        );
        let pediu_demais = TetoDeVideo::novo().com_escolha(Some(50_000_000));
        assert_ne!(
            pediu_demais.perna_que_aperta(SignalBand::Nominal),
            PernaQueAperta::Escolha
        );

        // E a razão bate com o número: a perna que aperta é a que o teto seguiu.
        assert_eq!(sala_cheia.teto(SignalBand::Nominal), Teto::Bps(300_000));
        assert_eq!(subida_ruim.teto(SignalBand::Nominal), Teto::Bps(360_000));
    }

    // -----------------------------------------------------------------------
    // O orçamento
    // -----------------------------------------------------------------------

    #[test]
    fn o_balde_repoe_com_o_tempo_e_nunca_passa_da_capacidade() {
        let inicio = Instant::now();
        // 800 kbps são 100 000 bytes por segundo.
        let mut balde = Balde::novo(800_000, inicio);
        assert!(balde.gastar(100_000, inicio), "o balde nasce cheio");
        assert!(!balde.gastar(1, inicio), "e vazio não empresta");

        let meio_segundo = inicio + Duration::from_millis(500);
        assert!(balde.gastar(50_000, meio_segundo));
        assert!(!balde.gastar(1, meio_segundo));

        // Uma hora parado não compra uma rajada de uma hora.
        let muito_depois = inicio + Duration::from_secs(3600);
        assert!(balde.gastar(100_000, muito_depois));
        assert!(!balde.gastar(1, muito_depois));
    }

    // -----------------------------------------------------------------------
    // O par QUIC
    // -----------------------------------------------------------------------

    /// Um par QUIC ligado, sem o handshake do produto.
    ///
    /// Mesma forma que `crate::frame::tests::par`, e pelo mesmo motivo: o que
    /// está sob teste é o transporte da tela, e um handshake no meio só
    /// acrescentaria maneiras de o teste falhar que não têm a ver com a
    /// pergunta.
    pub(crate) async fn par() -> (quinn::Connection, quinn::Connection) {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let certificado =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("certificado");
        let cadeia = vec![rustls::pki_types::CertificateDer::from(
            certificado.cert.der().to_vec(),
        )];
        let chave =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certificado.signing_key.serialize_der());

        let mut tls_servidor = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cadeia.clone(), chave.into())
            .expect("config do servidor");
        tls_servidor.alpn_protocols = vec![b"seele-test".to_vec()];
        let servidor = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(tls_servidor).expect("quic"),
        ));

        let escuta = quinn::Endpoint::server(servidor, SocketAddr::from(([127, 0, 0, 1], 0)))
            .expect("escutar");
        let endereco = escuta.local_addr().expect("endereço");

        let mut raiz = rustls::RootCertStore::empty();
        raiz.add(cadeia[0].clone()).expect("raiz");
        let mut tls_cliente = rustls::ClientConfig::builder()
            .with_root_certificates(raiz)
            .with_no_client_auth();
        tls_cliente.alpn_protocols = vec![b"seele-test".to_vec()];

        let mut cliente =
            quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0))).expect("cliente");
        cliente.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls_cliente).expect("quic"),
        )));

        let aceitando = tokio::spawn(async move {
            let conexao = escuta
                .accept()
                .await
                .expect("entrada")
                .await
                .expect("aceitar");
            (conexao, escuta)
        });

        let saida = cliente
            .connect(endereco, "localhost")
            .expect("conectar")
            .await
            .expect("conexão");
        let (entrada, escuta) = aceitando.await.expect("junção");

        // Vazam de propósito: derrubar os endpoints fecharia as conexões que o
        // teste ainda vai usar.
        std::mem::forget(cliente);
        std::mem::forget(escuta);
        (saida, entrada)
    }

    /// §3.1, e é a decisão mais cara da spec: **o vídeo vai num fluxo, e a voz
    /// continua nos datagramas dela.**
    ///
    /// `spikes/tela-no-transporte` mediu o que acontece quando os dois dividem
    /// a fila de datagramas do `quinn`: 16,1% da voz perdida e 2,16 s de atraso
    /// com o buffer padrão de 1 MiB, e **98,1% da voz descartada** ao encolher
    /// o buffer para 32 KiB — porque `send_datagram` põe voz e vídeo na mesma
    /// FIFO e descarta o mais velho.
    ///
    /// Este teste roda em loopback, onde não há gargalo para medir atraso.
    /// O que ele prende é o que sobrevive à ausência de gargalo: **nenhum byte
    /// de vídeo aparece como datagrama.** Troque o `open_uni` de
    /// `Transmissao::abrir` por `send_datagram` e ele fica vermelho na hora,
    /// porque o leitor de voz passa a receber quadros de tela.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_video_vai_no_fluxo_e_nunca_no_datagrama() {
        const VOZ: &[u8] = b"opus-20ms";
        const QUADROS: usize = 40;

        let (saida, entrada) = par().await;
        let agora = Instant::now();

        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 8_000_000, agora)
            .await
            .expect("abrir a transmissão");

        let ouvindo = tokio::spawn({
            let entrada = entrada.clone();
            async move {
                let mut recebidos = Vec::new();
                while recebidos.len() < QUADROS {
                    match entrada.read_datagram().await {
                        Ok(bytes) => recebidos.push(bytes.to_vec()),
                        Err(_) => break,
                    }
                }
                recebidos
            }
        });

        for numero in 0..QUADROS {
            // Um quadro de tela de 6 KiB, que é a ordem de grandeza de um
            // quadro comum de 1080p no teto de 1200 kbps.
            let quadro = vec![u8::try_from(numero % 251).unwrap_or_default(); 6 * 1024];
            transmissao
                .enviar_quadro(
                    &quadro,
                    false,
                    agora + Duration::from_millis(numero as u64 * 33),
                )
                .await
                .expect("enviar");
            saida
                .send_datagram(VOZ.to_vec().into())
                .expect("a voz sai por datagrama");
        }

        let voz = tokio::time::timeout(Duration::from_secs(5), ouvindo)
            .await
            .expect("a voz não chegou a tempo")
            .expect("junção");

        assert_eq!(voz.len(), QUADROS, "faltou voz");
        for datagrama in &voz {
            assert_eq!(
                datagrama, VOZ,
                "um datagrama trouxe algo que não é voz — o vídeo entrou na fila da voz"
            );
        }

        // E a tela chegou, inteira, pelo fluxo.
        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar a tela");
        assert_eq!(recepcao.cabecalho(), &cabecalho());
        for numero in 0..QUADROS {
            let quadro = recepcao
                .proximo_quadro()
                .await
                .expect("ler")
                .expect("o fluxo acabou cedo");
            assert!(!quadro.chave());
            assert_eq!(quadro.bytes.len(), 6 * 1024);
            assert_eq!(
                quadro.bytes.first().copied(),
                Some(u8::try_from(numero % 251).unwrap_or_default()),
                "os quadros chegaram fora de ordem"
            );
        }
    }

    /// §3.3: o quadro-chave sai espalhado, e sai **inteiro**.
    ///
    /// Espalhar leva o p95 da voz de 78,9 para 35,8 ms e o pior caso de 114,9
    /// para 42,7 ms, com o mesmo bitrate entregue — não se manda menos, manda-se
    /// em quatro tiques. Daí as duas metades deste teste: o remetente diz
    /// `Espalhando` e precisa de [`FATIAS_DO_QUADRO_CHAVE`] tiques para
    /// terminar, e quem recebe recebe o quadro-chave **byte por byte igual** ao
    /// que entrou, num quadro só.
    ///
    /// A segunda metade é a que não deixa espalhar virar formato: não há
    /// bandeira de continuação nem remontagem do outro lado, porque o fluxo é
    /// ordenado e espalhar é uma decisão de quando escrever.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_quadro_chave_sai_espalhado_e_chega_inteiro() {
        // 65 KiB é o quadro-chave de 1080p que `spikes/tela-no-codec` mediu, e
        // os **três bytes a mais** não são enfeite: um tamanho que divide certo
        // por [`FATIAS_DO_QUADRO_CHAVE`] esconde o defeito que este teste
        // existe para pegar. Com sobra, uma última fatia que não levasse o
        // resto deixaria bytes órfãos — e quem lê espera exatamente `tamanho`
        // bytes e ficaria pendurado para sempre por causa de uma divisão.
        let chave: Vec<u8> = (0..65 * 1024 + 3)
            .map(|i| u8::try_from(i % 256).unwrap_or(0))
            .collect();

        let (saida, entrada) = par().await;
        let inicio = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 8_000_000, inicio)
            .await
            .expect("abrir");

        let tique = |n: u64| inicio + Duration::from_millis(n * 33);

        assert_eq!(
            transmissao
                .enviar_quadro(&chave, true, tique(0))
                .await
                .expect("chave"),
            Envio::Espalhando,
            "o quadro-chave saiu num tique só"
        );

        // Os tiques seguintes carregam o resto dele, e os quadros comuns
        // entregues no meio são **descartados**: escritos ali, sairiam dentro
        // do quadro-chave, porque um fluxo QUIC é uma sequência ordenada.
        let comum = vec![9_u8; 4096];
        for numero in 1..FATIAS_DO_QUADRO_CHAVE {
            assert_eq!(
                transmissao
                    .enviar_quadro(&comum, false, tique(numero as u64))
                    .await
                    .expect("comum"),
                Envio::Descartado(MotivoDeDescarte::QuadroChaveEmVoo),
                "o tique {numero} deixou um quadro comum entrar no meio da chave"
            );
        }

        // Terminada a chave, a transmissão volta ao normal.
        assert_eq!(
            transmissao
                .enviar_quadro(&comum, false, tique(FATIAS_DO_QUADRO_CHAVE as u64))
                .await
                .expect("comum"),
            Envio::Enviado
        );

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        // Com prazo: uma fatia que não levasse o resto deixaria a leitura
        // pendurada em vez de errada, e um teste que trava é um teste que
        // ninguém lê o motivo.
        let recebida = tokio::time::timeout(Duration::from_secs(5), recepcao.proximo_quadro())
            .await
            .expect("o quadro-chave nunca terminou de chegar")
            .expect("ler")
            .expect("o fluxo acabou cedo");
        assert!(recebida.chave(), "chegou sem a marca de quadro-chave");
        assert_eq!(
            recebida.bytes, chave,
            "o quadro-chave espalhado não chegou igual ao que saiu"
        );

        let seguinte = recepcao
            .proximo_quadro()
            .await
            .expect("ler")
            .expect("o fluxo acabou cedo");
        assert_eq!(seguinte.bytes, comum);
    }

    /// A chave fecha **sem nenhum quadro novo**, que é a tela parada do §1.
    ///
    /// O defeito de campo: «a tela ficou travada pra mim que estou
    /// compartilhando e pra quem tá assistindo, em um frame só». O teste irmão
    /// acima faz a chave andar com quadros comuns chegando; este a faz andar sem
    /// nada chegando, que é a condição real — macOS e Windows só entregam
    /// quadro quando a imagem muda, e o controle de taxa do OpenH264 pula por
    /// conta própria mesmo quando ela muda.
    ///
    /// Sem [`Transmissao::escoar_chave`] não há segunda fatia: quem assiste
    /// nunca fecha um quadro, e quem compartilha também não vê nada, porque com
    /// chave em voo todo quadro que chega é descartado — o do espelho junto.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_quadro_chave_fecha_com_a_tela_parada() {
        // O mesmo tamanho com sobra do teste irmão, e pela mesma razão: uma
        // divisão exata esconderia bytes órfãos na última fatia.
        let chave: Vec<u8> = (0..65 * 1024 + 3)
            .map(|i| u8::try_from(i % 256).unwrap_or(0))
            .collect();

        let (saida, entrada) = par().await;
        let inicio = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 8_000_000, inicio)
            .await
            .expect("abrir");

        assert_eq!(
            transmissao
                .enviar_quadro(&chave, true, inicio)
                .await
                .expect("chave"),
            Envio::Espalhando
        );

        // Os tiques da tela parada. Nenhum quadro entra por aqui — é só o laço
        // acordando no ritmo dele e encontrando a captura sem nada para dar.
        for numero in 1..FATIAS_DO_QUADRO_CHAVE {
            transmissao
                .escoar_chave()
                .await
                .unwrap_or_else(|erro| panic!("escoar a fatia {numero}: {erro}"));
        }

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        let recebida = tokio::time::timeout(Duration::from_secs(5), recepcao.proximo_quadro())
            .await
            .expect("o quadro-chave nunca terminou de chegar com a tela parada")
            .expect("ler")
            .expect("o fluxo acabou cedo");
        assert!(recebida.chave(), "chegou sem a marca de quadro-chave");
        assert_eq!(
            recebida.bytes, chave,
            "o quadro-chave não chegou igual ao que saiu"
        );

        // E o fluxo voltou ao normal: o próximo quadro de verdade não é mais
        // descartado por haver chave em voo.
        let comum = vec![9_u8; 4096];
        assert_eq!(
            transmissao
                .enviar_quadro(&comum, false, inicio + Duration::from_millis(200))
                .await
                .expect("comum"),
            Envio::Enviado,
            "a chave não soltou o fluxo depois de escoada"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_quadro_acima_do_teto_e_descartado_e_nao_enfileirado() {
        // §1, e é a mesma decisão que `specs/03-audio.md` já tomou para o
        // áudio: um quadro velho entregue tarde é pior que um quadro perdido.
        // Enfileirar aqui seria pôr bytes na fila do gargalo, que é exatamente
        // o que o §3.2 mediu custando 200 ms de voz.
        let (saida, _entrada) = par().await;
        let agora = Instant::now();
        // 80 kbps: 10 000 bytes por segundo de orçamento.
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 80_000, agora)
            .await
            .expect("abrir");

        assert_eq!(
            transmissao
                .enviar_quadro(&[1_u8; 9_000], false, agora)
                .await
                .expect("cabe"),
            Envio::Enviado
        );
        assert_eq!(
            transmissao
                .enviar_quadro(&[2_u8; 9_000], false, agora)
                .await
                .expect("não cabe"),
            Envio::Descartado(MotivoDeDescarte::AcimaDoTeto),
            "o segundo quadro do mesmo segundo passou do teto e saiu assim mesmo"
        );

        // Um segundo depois o orçamento voltou.
        assert_eq!(
            transmissao
                .enviar_quadro(&[3_u8; 9_000], false, agora + Duration::from_secs(1))
                .await
                .expect("cabe de novo"),
            Envio::Enviado
        );

        let (enviados, descartados, _) = transmissao.contagem();
        assert_eq!((enviados, descartados), (2, 1));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn baixar_o_teto_nao_devolve_fichas() {
        // Uma queda de faixa que liberasse rajada seria o oposto exato do que a
        // queda de faixa quer dizer: o sinal piorou, e o vídeo passaria a poder
        // mandar mais de uma vez.
        let (saida, _entrada) = par().await;
        let agora = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 800_000, agora)
            .await
            .expect("abrir");

        assert_eq!(
            transmissao
                .enviar_quadro(&[1_u8; 99_000], false, agora)
                .await
                .expect("cabe"),
            Envio::Enviado
        );
        // O sinal caiu de faixa: o teto vai à metade.
        transmissao.ajustar_teto(400_000, agora);
        assert_eq!(
            transmissao
                .enviar_quadro(&[2_u8; 1_000], false, agora)
                .await
                .expect("não cabe"),
            Envio::Descartado(MotivoDeDescarte::AcimaDoTeto)
        );
    }

    /// O som nunca é escrito no meio de um quadro-chave.
    ///
    /// **É o defeito que parou a transmissão inteira no primeiro quadro.** Um
    /// quadro-chave sai em quatro fatias, uma por tique, e o cabeçalho anuncia o
    /// tamanho **inteiro**: quem lê conta bytes até fechar a conta. Um cabeçalho
    /// de som escrito no meio disso vira payload de imagem — o quadro sai
    /// errado, o enquadramento perde o passo, e tudo depois é lixo.
    ///
    /// Em campo: «exibe apenas 1 frame e o Windows não consegue ver», sem som
    /// nenhum, porque o som tinha sido lido como imagem.
    ///
    /// O teste lê o outro lado do fio e exige que os quadros voltem inteiros e
    /// na ordem certa. Sem a fila, o segundo quadro volta corrompido ou o
    /// enquadramento estoura.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_som_nao_atravessa_um_quadro_chave_pela_metade() {
        let (saida, entrada) = par().await;
        let agora = Instant::now();
        // Teto largo: o que se mede aqui é a ordem no fio, não o balde.
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 800_000_000, agora)
            .await
            .expect("abrir");

        // Um quadro-chave grande o bastante para sair em quatro fatias, e som
        // empurrado **entre** elas — que é o que a bomba faz a cada tique.
        let chave = vec![7_u8; 40_000];
        assert_eq!(
            transmissao
                .enviar_quadro(&chave, true, agora)
                .await
                .expect("chave"),
            Envio::Espalhando
        );
        for _ in 0..FATIAS_DO_QUADRO_CHAVE {
            transmissao.enviar_som(&[1, 2, 3, 4]).await.expect("som");
            let _ = transmissao
                .enviar_quadro(&[9_u8; 100], false, agora)
                .await
                .expect("tique");
        }

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        let primeiro = recepcao
            .proximo_quadro()
            .await
            .expect("ler")
            .expect("o quadro-chave");
        assert_eq!(
            primeiro.tipo,
            TipoDeQuadro::Chave,
            "o primeiro quadro do fluxo não é o quadro-chave"
        );
        assert_eq!(
            primeiro.bytes, chave,
            "o quadro-chave voltou diferente do que saiu: alguma coisa foi \
             escrita no meio das fatias dele"
        );

        // E o que vem depois é som, inteiro — não os bytes de um quadro cortado.
        let segundo = recepcao
            .proximo_quadro()
            .await
            .expect("ler")
            .expect("o que veio depois");
        assert_eq!(segundo.tipo, TipoDeQuadro::Som);
        assert_eq!(segundo.bytes, vec![1, 2, 3, 4]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_quadro_grande_demais_ou_vazio_nao_sai() {
        let (saida, _entrada) = par().await;
        let agora = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 800_000_000, agora)
            .await
            .expect("abrir");
        assert_eq!(
            transmissao
                .enviar_quadro(&vec![0_u8; MAX_QUADRO_LEN + 1], false, agora)
                .await
                .expect("recusa não é erro"),
            Envio::Descartado(MotivoDeDescarte::GrandeDemais)
        );
        assert_eq!(
            transmissao
                .enviar_quadro(&[], false, agora)
                .await
                .expect("vazio"),
            Envio::Descartado(MotivoDeDescarte::Vazio)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_tamanho_absurdo_do_par_e_recusado_antes_de_alocar() {
        // `specs/08-seguranca.md`, e conferido no receptor e não só no emissor:
        // um par escreve os cinco bytes de cabeçalho à mão e pula o emissor
        // inteiro. Ler um tamanho de 4 GiB e reservar por ele é a negação de
        // serviço mais velha que existe.
        let (saida, entrada) = par().await;
        let mut fluxo = saida.open_uni().await.expect("abrir na mão");
        // Este teste finge ser o remetente, e remetente escreve o tipo do
        // fluxo antes do cabeçalho — ver o §5.2 da spec. Sem esta linha ele
        // reprovava a leitura por não achar o que ele mesmo não mandou.
        fluxo
            .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
            .await
            .expect("tipo do fluxo");
        let mut abertura = [0_u8; SCREEN_HEADER_LEN];
        cabecalho().encode(&mut abertura).expect("cabeçalho");
        fluxo.write_all(&abertura).await.expect("abertura");
        fluxo
            .write_all(&escrever_cabecalho_de_quadro(TipoDeQuadro::Comum, u32::MAX))
            .await
            .expect("quadro absurdo");

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        assert_eq!(
            recepcao.proximo_quadro().await,
            Err(ErroDeTela::QuadroGrandeDemais {
                len: u32::MAX as usize
            }),
            "um tamanho absurdo virou uma alocação de 4 GiB"
        );

        // E o zero, pelo mesmo motivo que `ScreenHeader::check` recusa um lado
        // de zero: é muito mais vezes uma captura que falhou do que uma
        // escolha, e não há quadro atrás dele de qualquer jeito.
        let mut vazio = saida.open_uni().await.expect("abrir na mão");
        vazio
            .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
            .await
            .expect("tipo do fluxo");
        vazio.write_all(&abertura).await.expect("abertura");
        vazio
            .write_all(&escrever_cabecalho_de_quadro(TipoDeQuadro::Comum, 0))
            .await
            .expect("quadro vazio");
        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        assert_eq!(
            recepcao.proximo_quadro().await,
            Err(ErroDeTela::QuadroVazio)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn o_fim_do_fluxo_e_o_fim_da_transmissao_e_nao_um_erro() {
        // Toda transmissão termina por aqui. Tratar o fim limpo como erro faria
        // quem assistiu até o fim ver uma mensagem de falha.
        let (saida, entrada) = par().await;
        let agora = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 800_000, agora)
            .await
            .expect("abrir");
        transmissao
            .enviar_quadro(&[7_u8; 128], false, agora)
            .await
            .expect("um quadro");
        transmissao.encerrar();

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        assert!(recepcao.proximo_quadro().await.expect("ler").is_some());
        assert_eq!(
            recepcao.proximo_quadro().await.expect("fim limpo"),
            None,
            "o fim do fluxo virou erro"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn uma_resolucao_fora_do_teto_nao_abre_fluxo_nenhum() {
        // Conferida antes do `open_uni`: uma resolução que a prova não cobre
        // não vale um fluxo aberto que só será fechado. §6 item 10 põe tudo
        // acima de 1080p fora da v1, e o §2 mediu a CPU só até ali.
        let (saida, _entrada) = par().await;
        let grande = ScreenHeader {
            width: 1920,
            height: 1920,
            ..cabecalho()
        };
        let erro = Transmissao::abrir(&saida, grande, 800_000, Instant::now())
            .await
            .expect_err("aceitou 1920×1920");
        assert!(matches!(erro, ErroDeTela::Cabecalho(_)));
    }
}

#[cfg(test)]
mod o_eixo_da_degradacao {
    use super::{
        cadencia_para, resolucao_para, Prioridade, FRACAO_DO_CAMINHO, TETO_ESTIMADO_PARA_1080P_BPS,
        TETO_ESTIMADO_PARA_720P_BPS,
    };
    use seele_video::codec::{Cadencia, Resolucao};

    /// **A metade do §2 que não existia.**
    ///
    /// A regra é «a resolução segura, o quadro cede», e só a primeira metade
    /// estava implementada: a cadência era a escolha de quem compartilha, 30 por
    /// padrão, e nada a reduzia quando o orçamento apertava.
    ///
    /// Isso funcionava por acidente enquanto o codificador era o do Cisco, que
    /// joga quadro fora sozinho — a tabela de onde saíram os limiares mostra
    /// 11% a 16% de quadros perdidos. O codec do sistema não joga nenhum e borra
    /// todos, e foi assim que chegou: «assistindo a transmissão do mac, a imagem
    /// fica borrada e blocada».
    #[test]
    fn o_quadro_cede_quando_a_resolucao_nao_tem_bits() {
        // Nos limiares, a cadência cheia cabe — é de lá que os números vêm.
        assert_eq!(
            cadencia_para(
                TETO_ESTIMADO_PARA_1080P_BPS,
                Resolucao::P1080,
                Prioridade::Nitidez,
                Cadencia::Q30
            ),
            Cadencia::Q30,
            "no limiar de 1080p os 30 quadros são exatamente o que a régua compra"
        );

        // E abaixo dele o quadro cede em vez de a imagem borrar. Metade do
        // limiar de 720p compra 720p a 15 quadros, e não a 30.
        //
        // **O número era 900 kbps**, e mudou junto com a régua: aquele valor era
        // o que 720p *gastou* no teto de 1200 kbps, num ponto em que o
        // codificador entregava 0,033 bits por pixel. Metade do limiar é a mesma
        // frase — «abaixo do limiar, o quadro cede» — dita contra a tabela que
        // vale.
        assert_eq!(
            cadencia_para(
                TETO_ESTIMADO_PARA_720P_BPS / 2,
                Resolucao::P720,
                Prioridade::Nitidez,
                Cadencia::Q30
            ),
            Cadencia::Q15,
            "metade dos bits do limiar tem de comprar metade dos quadros"
        );
        assert_eq!(
            cadencia_para(300_000, Resolucao::P540, Prioridade::Nitidez, Cadencia::Q30),
            Cadencia::Q8,
            "o §2 nomeia 8 quadros como o ponto em que texto ainda se lê"
        );
    }

    #[test]
    fn a_escolha_continua_sendo_teto_e_nunca_piso() {
        // Como a resolução: o que sai é o menor entre o pedido e o que cabe.
        // Quem escolheu 8 quadros continua em 8 numa fibra — a função não
        // promove ninguém.
        assert_eq!(
            cadencia_para(
                50_000_000,
                Resolucao::P1080,
                Prioridade::Nitidez,
                Cadencia::Q8
            ),
            Cadencia::Q8,
            "uma fibra promoveu quem tinha escolhido 8 quadros"
        );
        assert_eq!(
            cadencia_para(
                50_000_000,
                Resolucao::P1080,
                Prioridade::Nitidez,
                Cadencia::Q60
            ),
            Cadencia::Q60,
            "e quem escolheu 60 e tem banda continua em 60"
        );
    }

    #[test]
    fn movimento_cede_quadro_quando_o_teto_deixaria_de_ser_teto() {
        // **Não é o eixo mudando de ideia.** Movimento continua não cedendo
        // quadro por qualidade — o teste vizinho prova isso e continua verde.
        // O que ele cede é o ponto em que o codificador do sistema para de
        // respeitar o teto, medido entre 0,048 e 0,077 bits por pixel, e ali a
        // conta deixa de ser sobre imagem e passa a ser sobre a voz (§3.2).
        //
        // A 1,2 Mbps, movimento pede 540p; a 60 quadros isso são 0,039 bpp, e
        // foi a linha que entregou 135% do teto.
        let apertado = 1_200_000;
        assert_eq!(
            resolucao_para(apertado, Prioridade::Movimento),
            Resolucao::P540
        );
        let cedeu = cadencia_para(
            apertado,
            Resolucao::P540,
            Prioridade::Movimento,
            Cadencia::Q60,
        );
        assert!(
            cedeu.hz() < 60,
            "movimento ficou em {} quadros num orçamento onde o teto deixa de \
             valer, e teto furado é a voz cedendo à tela",
            cedeu.hz()
        );

        // E onde o teto se sustenta, movimento continua intocado: 60 quadros.
        assert_eq!(
            cadencia_para(
                3_000_000,
                Resolucao::P540,
                Prioridade::Movimento,
                Cadencia::Q60
            ),
            Cadencia::Q60,
            "movimento perdeu quadro num orçamento que o codificador respeita"
        );
    }

    #[test]
    fn movimento_nao_paga_duas_vezes_pela_mesma_falta_de_banda() {
        // **Não é exceção, é a definição do eixo.** «O quadro segura, a
        // resolução cede» — e a resolução já cedeu em `resolucao_para_movimento`,
        // que cobra o dobro por degrau. Cortar quadro aqui cobraria duas vezes,
        // e a 8 quadros um jogo não é pior: é inutilizável.
        // **Os tetos subiram, e a frase não.** Eram 300 k, 900 k e 1,5 M.
        // Abaixo de 1,2 Mbps a 540p o orçamento por quadro entra na faixa em que
        // o codificador do sistema para de respeitar o teto, e ali quem corta é
        // `movimento_cede_quadro_quando_o_teto_deixaria_de_ser_teto` — por
        // transporte, não por qualidade. Este teste é sobre a outra coisa: onde
        // o teto se sustenta, movimento não perde um quadro sequer.
        for teto in [1_200_000_u32, 1_500_000, 3_000_000] {
            let resolucao = resolucao_para(teto, Prioridade::Movimento);
            assert_eq!(
                cadencia_para(teto, resolucao, Prioridade::Movimento, Cadencia::Q30),
                Cadencia::Q30,
                "a {teto} bps, movimento perdeu quadro além da resolução que já cedeu"
            );
        }
    }

    /// O mesmo teto compra resoluções diferentes conforme o que se protege.
    ///
    /// É o eixo inteiro num teste: seis megabits compram 1080p para quem mostra
    /// texto e 720p para quem joga — e os bits que a segunda não gastou em
    /// pixels vão para o quadro, que é o que jogo precisa.
    #[test]
    fn movimento_troca_um_degrau_de_resolucao_por_quadros() {
        for teto in [1_500_000, 3_000_000, 6_000_000, 50_000_000] {
            let nitida = resolucao_para(teto, Prioridade::Nitidez);
            let movida = resolucao_para(teto, Prioridade::Movimento);
            assert!(
                movida.altura() <= nitida.altura(),
                "a {teto} bps, movimento pediu {movida:?} e nitidez pediu {nitida:?} — \
                 movimento nunca pede mais resolução que nitidez"
            );
        }
        // **A afirmação de cima é a invariante; esta era a regra que a
        // implementava, e ela mudou.**
        //
        // Aqui estava escrito que a 6 Mbps movimento pede 720p — porque a regra
        // era «um degrau abaixo, sempre». Ela cumpria a intenção quando o teto
        // apertava e a estragava quando não apertava: `Movimento` nunca
        // alcançava 1080p, em banda nenhuma, e como ele virou o padrão, 1080p
        // ficou inalcançável no produto inteiro.
        //
        // A regra agora é limiar e não degrau: o dobro do preço para a mesma
        // resolução. O que a invariante do laço acima guarda continua valendo —
        // movimento nunca pede **mais** que nitidez —, e o que muda é que a 6
        // Mbps o dobro de 1,5 já cabe.
        //
        // **Os tetos deste teste andaram com a régua, e o que eles dizem não.**
        // Movimento paga o dobro por degrau, então o teto que prova «movimento
        // alcança 1080p» é, por construção, o dobro do limiar de 1080p — antes
        // 3 Mbps, agora 12,42. Escrever 6 000 000 aqui de novo provaria o
        // contrário do que a frase diz.
        let dois_1080p = TETO_ESTIMADO_PARA_1080P_BPS * 2;
        assert_eq!(
            resolucao_para(dois_1080p, Prioridade::Nitidez),
            Resolucao::P1080
        );
        assert_eq!(
            resolucao_para(dois_1080p, Prioridade::Movimento),
            Resolucao::P1080,
            "no dobro do limiar de 1080p, movimento alcança 1080p"
        );

        // E o aperto continua apertando, que é a razão de o eixo existir: um
        // teto que compra 720p para quem mostra texto compra 540p para quem
        // joga, porque movimento cobra o dobro.
        assert_eq!(
            resolucao_para(TETO_ESTIMADO_PARA_720P_BPS, Prioridade::Nitidez),
            Resolucao::P720
        );
        assert_eq!(
            resolucao_para(TETO_ESTIMADO_PARA_720P_BPS, Prioridade::Movimento),
            Resolucao::P540,
            "no limiar de 720p, movimento continua cedendo resolução para segurar o quadro"
        );

        // **E 1080p tem de ser alcançável.** Sem esta linha, a regra pode voltar
        // a ser um teto disfarçado de degrau e nada reprova.
        assert_eq!(
            resolucao_para(u32::MAX, Prioridade::Movimento),
            Resolucao::P1080,
            "movimento não alcança 1080p nem com banda infinita: a regra virou \
             um teto, e foi assim que 1080p ficou impossível no produto"
        );
    }

    /// O piso é o piso nos dois eixos.
    ///
    /// 540p é o fundo da lista do §5, e movimento não inventa um quarto degrau
    /// para descer: quem nem a 540p sustenta o quadro está num caminho que o
    /// piso de banda vai parar, com motivo enumerado.
    #[test]
    fn movimento_nao_desce_abaixo_do_piso_da_lista() {
        assert_eq!(
            resolucao_para(1, Prioridade::Movimento),
            Resolucao::P540,
            "movimento inventou um degrau abaixo do piso do §5"
        );
    }

    /// O padrão é o do §2, e isto é uma decisão e não um acaso do `Default`.
    ///
    /// Compartilhar tela ainda é, na maioria das vezes, mostrar uma tela. Quem
    /// não escolher nada continua recebendo a regra que a spec fechou.
    #[test]
    fn o_padrao_continua_sendo_o_do_paragrafo_dois() {
        assert_eq!(Prioridade::default(), Prioridade::Nitidez);
        assert_eq!(
            resolucao_para(6_000_000, Prioridade::default()),
            resolucao_para(6_000_000, Prioridade::Nitidez)
        );
    }

    /// Quantos bits por pixel um degrau entrega no seu próprio limiar.
    ///
    /// É a conta que decide se sai bloco, e a única que atravessa resolução e
    /// cadência ao mesmo tempo: um limiar só é honesto se, exatamente em cima
    /// dele, a imagem que ele promete couber nos bits que ele tem.
    fn bits_por_pixel(teto_bps: u32, resolucao: Resolucao, quadros: u32) -> f64 {
        let pixels = (resolucao.largura() * resolucao.altura()) as f64;
        f64::from(teto_bps) / f64::from(quadros) / pixels
    }

    /// 0,10 bits por pixel é onde a borda de uma fonte para de virar bloco.
    const PISO_BPP: f64 = 0.10;

    #[test]
    fn cada_limiar_compra_a_resolucao_que_promete() {
        // Em cima do limiar, e não acima: o limiar é o pior caso do degrau, e é
        // ele que tem de fechar a conta. Se fechar só com folga, o degrau está
        // prometendo o que não entrega na hora em que alguém de fato o alcança.
        for (teto, resolucao) in [
            (TETO_ESTIMADO_PARA_1080P_BPS, Resolucao::P1080),
            (TETO_ESTIMADO_PARA_720P_BPS, Resolucao::P720),
        ] {
            let cadencia = cadencia_para(teto, resolucao, Prioridade::Nitidez, Cadencia::Q30);
            let bpp = bits_por_pixel(teto, resolucao, cadencia.hz());
            assert!(
                bpp >= PISO_BPP,
                "{resolucao:?} no limiar de {teto} bps a {} quadros dá {bpp:.3} bpp, \
                 abaixo do piso de {PISO_BPP} — é o limiar prometendo o que não paga",
                cadencia.hz(),
            );
        }
    }

    #[test]
    fn o_teto_da_estimativa_alcanca_os_oito_megabits_que_o_produto_quer() {
        // O vídeo leva FRACAO_DO_CAMINHO — 60% — do caminho, então o caminho
        // tem de chegar a 13,3 Mbps para o vídeo chegar a 8. Sem esta folga,
        // «quero 8 Mbps» é uma escolha que o `min()` das três pernas do §5.1
        // derruba em silêncio.
        let maior_video_bps =
            u64::from(crate::caminho::TETO_DA_ESTIMATIVA_BPS) * u64::from(FRACAO_DO_CAMINHO) / 100;
        assert!(
            maior_video_bps >= 8_000_000,
            "o teto da estimativa só chega a {maior_video_bps} bps de vídeo, e o produto quer 8 M"
        );
    }

    #[test]
    fn o_degrau_de_cima_da_escada_e_alcancavel() {
        // **1080p a 60 quadros tem de ser possível em alguma rede.** O §5 fecha
        // a lista de degraus, e um degrau que nenhuma banda alcança não é um
        // degrau: é uma opção que mente na interface.
        //
        // Isto reprovou duas vezes. Com o teto da estimativa em 10 Mbps e depois
        // em 14, `cadencia_para` nunca chegava a Q60 em 1080p — 8,4 Mbps ÷ 208 k
        // dão 40, que compra Q30 —, então o topo da lista era inalcançável por
        // construção, inclusive numa LAN de 10 gigabits. É a mesma doença que
        // `movimento_troca_um_degrau_de_resolucao_por_quadros` já tinha nomeado
        // uma vez: «a regra virou um teto, e foi assim que 1080p ficou
        // impossível no produto».
        let teto_maximo = crate::caminho::TETO_DA_ESTIMATIVA_BPS / 100 * FRACAO_DO_CAMINHO;
        assert_eq!(
            resolucao_para(teto_maximo, Prioridade::Nitidez),
            Resolucao::P1080
        );
        assert_eq!(
            cadencia_para(
                teto_maximo,
                Resolucao::P1080,
                Prioridade::Nitidez,
                Cadencia::Q60
            ),
            Cadencia::Q60,
            "no caminho mais largo que a sonda afirma, 1080p60 tem de caber"
        );
        assert!(
            bits_por_pixel(teto_maximo, Resolucao::P1080, 60) >= PISO_BPP,
            "1080p60 no teto máximo ainda sai abaixo do piso de bits por pixel"
        );
    }

    #[test]
    fn oito_megabits_compram_1080p_a_trinta_com_folga() {
        // A ponta de cima da escada, escrita para não voltar a ser suposição: o
        // que o pedido de 8 Mbps de fato entrega na tela.
        let resolucao = resolucao_para(8_000_000, Prioridade::Nitidez);
        assert_eq!(resolucao, Resolucao::P1080);
        let cadencia = cadencia_para(8_000_000, resolucao, Prioridade::Nitidez, Cadencia::Q30);
        assert_eq!(cadencia, Cadencia::Q30);
        assert!(bits_por_pixel(8_000_000, resolucao, cadencia.hz()) >= 0.12);
    }
}

#[cfg(test)]
mod o_som_no_mesmo_fluxo {
    //! O som viaja **dentro** do fluxo da tela, como um terceiro tipo de quadro.
    //!
    //! Dois fluxos separados chegariam em ordens diferentes e precisariam de
    //! carimbo e de fila de alinhamento; no mesmo fluxo, a ordem de chegada é a
    //! ordem de saída. E o orçamento de banda da tela já mede o fluxo inteiro —
    //! um segundo fluxo precisaria de um segundo teto e de uma segunda decisão
    //! sobre o que cede.

    use super::{TipoDeQuadro, CABECALHO_DE_QUADRO_LEN};

    #[test]
    fn os_tres_tipos_vao_e_voltam() {
        for tipo in [TipoDeQuadro::Comum, TipoDeQuadro::Chave, TipoDeQuadro::Som] {
            assert_eq!(
                TipoDeQuadro::de_byte(tipo.byte()),
                Some(tipo),
                "o byte de {tipo:?} não volta como {tipo:?}"
            );
        }
    }

    #[test]
    fn um_byte_que_nao_e_dos_tres_nao_vira_tipo_nenhum() {
        // Nunca «pular pelo tamanho»: o tamanho é o único número que um fluxo
        // de lixo controla, e confiar nele para pular é pedir a alocação que
        // ele quiser.
        for byte in [3_u8, 4, 200, u8::MAX] {
            assert_eq!(
                TipoDeQuadro::de_byte(byte),
                None,
                "o byte {byte} virou tipo"
            );
        }
    }

    #[test]
    fn so_a_imagem_chave_e_porta_de_entrada() {
        // Quem chega no meio precisa de uma imagem que baste a si mesma. Som
        // não começa imagem nenhuma, e entrar por ele entregaria imagem pela
        // metade — é a mesma regra que o `Enquadramento` do servidor guarda.
        assert!(TipoDeQuadro::Chave.e_chave());
        assert!(!TipoDeQuadro::Comum.e_chave());
        assert!(
            !TipoDeQuadro::Som.e_chave(),
            "um quadro de som virou porta de entrada"
        );
    }

    #[test]
    fn o_cabecalho_do_som_leva_o_tipo_e_o_tamanho() {
        let cabecalho = super::escrever_cabecalho_de_quadro(TipoDeQuadro::Som, 4);
        assert_eq!(cabecalho.len(), CABECALHO_DE_QUADRO_LEN);
        assert_eq!(cabecalho[0], TipoDeQuadro::Som.byte());
        assert_eq!(&cabecalho[1..], &4_u32.to_be_bytes());
    }
}
