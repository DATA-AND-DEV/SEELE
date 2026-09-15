//! A troca de aparelho feita **no sistema operacional**, exercida por
//! comportamento.
//!
//! # O defeito que este arquivo existe para acusar
//!
//! Trocar de microfone ou de fone só valia depois de reiniciar o aplicativo. O
//! caminho de troca *dentro* da tela do SEELE sempre esteve certo; o que não
//! existia era reação à troca feita **fora** dele — na bandeja do Windows, nas
//! Configurações do Mac, ou puxando o fone da tomada. O `cpal` avisava dos três
//! casos pelo retorno de erro do fluxo, o produto contava o aviso como «mais
//! um erro» e jogava fora o tipo dele, e o laço seguia falando com um aparelho
//! que já não era o de ninguém. Reiniciar era o único momento em que o padrão
//! do sistema voltava a ser resolvido.
//!
//! # Por que não há placa de som aqui, e por que isso não invalida o teste
//!
//! Nenhuma máquina de CI tem duas placas de som para trocar, e nenhuma tem uma
//! tomada para puxar um fone. O que este teste finge é **só** a abertura do
//! aparelho. Tudo o que decidia errado é o de verdade: o erro é um
//! `cpal::Error` construído com o mesmo `ErrorKind` que o backend entrega, a
//! classificação é a de produção, os contadores são os que a thread de tempo
//! real escreve, a máquina de estados é a que roda no laço, e o painel que a
//! interface lê é o mesmo tipo que a sessão publica.
//!
//! Guarda de texto-fonte não serve para isto: um `include_str!` procurando a
//! palavra «supervisor» ficava verde mesmo quando o supervisor inteiro era
//! código morto — que foi exatamente o estado em que este defeito viveu.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use seele_audio::device::retorno_de_erro;
use seele_audio::playout::PlayoutClock;
use seele_audio::rt::StreamCounters;
use seele_audio::supervisor::{AvisoDeAparelho, Reabertura};
use seele_core::{
    seguir_o_aparelho, Acompanhamento, AparelhosAbertos, CaptureDevice, DeviceRates,
    EstadoDoAparelho, EstadoDoAudio, PassoDoAparelho, PlaybackDevice,
};

/// O quadro de voz, como `specs/03-audio.md` o define.
const QUADRO_MS: u32 = 20;

/// A máquina de quem usa: o que ela oferece como padrão, agora.
///
/// `None` é o instante real entre o fone sair da tomada e o sistema eleger
/// outro padrão — e é nesse instante que o produto precisava continuar
/// tentando em vez de desistir calado.
struct MaquinaDaPessoa {
    padrao: Option<&'static str>,
    aberturas: u32,
}

/// O que uma abertura entrega ao laço, no lugar do `AudioIo` de verdade.
///
/// Ele traz os **próprios contadores**, e isso não é detalhe de teste: em
/// produção cada `device::open` cria um `StreamCounters` novo e zerado, o laço
/// troca o `AudioIo` inteiro pelo que a reabertura entregou, e a volta seguinte
/// lê desses contadores. Um teste que reaproveitasse um único jogo de
/// contadores entre aberturas nunca veria o defeito de a segunda troca da
/// sessão ser ignorada.
struct AparelhoDeMentira {
    nome: &'static str,
    /// A taxa nativa dele. Aparelhos de verdade divergem nisso — um fone de USB
    /// a 44,1 kHz no lugar de uma placa a 48 kHz é o caso comum — e é por isso
    /// que ela tem de seguir a troca junto com o nome.
    hz: u32,
    contadores: std::sync::Arc<StreamCounters>,
}

impl AparelhoDeMentira {
    fn novo(nome: &'static str) -> Self {
        // Taxa tirada do nome para que cada aparelho deste teste tenha a sua,
        // sem uma tabela à parte que pudesse discordar de quem o abriu.
        let hz = match nome {
            // O aparelho que abre e com o qual não há laço possível. Existe
            // porque é um caminho de produção — o reamostrador recusar as taxas
            // do que abriu — e sem ele esse caminho não tem teste nenhum.
            "aparelho-de-taxa-impossivel" => 0,
            outro if outro.contains("usb") => 44_100,
            _ => 48_000,
        };
        Self {
            nome,
            hz,
            contadores: StreamCounters::shared(),
        }
    }
}

impl AparelhosAbertos for AparelhoDeMentira {
    fn microfone(&self) -> Option<CaptureDevice> {
        Some(CaptureDevice {
            id: format!("id:{}", self.nome),
            name: self.nome.to_owned(),
            default: true,
        })
    }

    fn saida(&self) -> Option<PlaybackDevice> {
        Some(PlaybackDevice {
            id: format!("id:{}", self.nome),
            name: self.nome.to_owned(),
            default: true,
        })
    }

    fn taxas(&self) -> DeviceRates {
        DeviceRates {
            capture_hz: self.hz,
            playback_hz: self.hz,
        }
    }

    /// Cem milissegundos de anel, na taxa **deste** aparelho.
    ///
    /// Amarrada à taxa de propósito: é o que faz a troca para um aparelho de
    /// 44,1 kHz mudar o anel junto com o reamostrador, como em produção.
    fn capacidade_de_saida(&self) -> usize {
        (self.hz as usize / 10).max(1)
    }
}

impl Reabertura for MaquinaDaPessoa {
    type Aberto = AparelhoDeMentira;
    type Erro = &'static str;

    fn reabrir(&mut self) -> Result<Self::Aberto, Self::Erro> {
        self.aberturas += 1;
        self.padrao
            .map(AparelhoDeMentira::novo)
            .ok_or("a máquina não está oferecendo aparelho nenhum")
    }

    fn aviso_de(&self, aberto: &Self::Aberto) -> AvisoDeAparelho {
        aberto.contadores.aviso_de_aparelho()
    }
}

/// Uma sessão de voz, do ponto de vista do aparelho.
///
/// Junta as três peças exatamente como o laço de áudio as junta: os contadores
/// que a thread de tempo real escreve, o acompanhamento que decide, e o painel
/// que a interface lê.
struct SessaoDeVoz {
    /// Os contadores do aparelho **em uso agora**. Trocam junto com ele, como
    /// em produção: o laço larga o `AudioIo` antigo e passa a ler o novo.
    contadores: std::sync::Arc<StreamCounters>,
    acompanhamento: Acompanhamento,
    painel: Mutex<EstadoDoAudio>,
    maquina: MaquinaDaPessoa,
    /// As quatro filas de amostras que o laço tem a caminho.
    ///
    /// Elas estão na taxa do aparelho de **antes**, e é por isso que a troca
    /// tem de esvaziá-las: tocadas no aparelho de agora, saem como um estalo no
    /// primeiro instante dele.
    restos: [Vec<f32>; 4],
    /// O relógio de reprodução do laço, que a reabertura reacerta.
    relogio: PlayoutClock,
    comecou: Instant,
    /// O aparelho abriu e o reamostrador recusou as taxas dele: o laço acaba.
    sem_laco_possivel: bool,
}

impl SessaoDeVoz {
    fn comecou_em(aparelho: &'static str) -> Self {
        let inicial = AparelhoDeMentira::novo(aparelho);
        Self {
            contadores: std::sync::Arc::clone(&inicial.contadores),
            acompanhamento: Acompanhamento::novo(),
            painel: Mutex::new(EstadoDoAudio {
                capture: inicial.microfone(),
                playback: inicial.saida(),
                estado: EstadoDoAparelho::Funcionando,
                reaberturas: 0,
                taxas: inicial.taxas(),
            }),
            maquina: MaquinaDaPessoa {
                padrao: Some(aparelho),
                aberturas: 0,
            },
            restos: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            relogio: PlayoutClock::new(Instant::now(), QUADRO_MS),
            comecou: Instant::now(),
            sem_laco_possivel: false,
        }
    }

    /// O que o `cpal` faz quando o sistema mexe no aparelho: chama o retorno de
    /// erro do fluxo, na thread de tempo real, com um erro que tem um tipo.
    fn o_cpal_avisa(&self, tipo: cpal::ErrorKind) {
        // Pelo mesmo fechamento que o `cpal` recebe ao montar o fluxo, e não
        // pela classificação direta: entrar por `classificar` saltaria o ponto
        // exato onde o erro era jogado fora, e este teste ficaria verde com o
        // defeito de volta.
        let mut retorno = retorno_de_erro(std::sync::Arc::clone(&self.contadores));
        retorno(cpal::Error::new(tipo));
    }

    /// Uma volta do laço de áudio.
    ///
    /// Pela **mesma função que o laço de produção chama** —
    /// [`seguir_o_aparelho`] —, e não por uma volta refeita à mão. A diferença
    /// importa: refeita à mão, este teste provava o ciclo e o teste, não a
    /// composição que a pessoa ouve (ler o aviso, reabrir, redimensionar,
    /// esvaziar o que era do aparelho antigo e reacertar o relógio, nessa
    /// ordem). O que fica fora do alcance de qualquer teste é só o `cpal`.
    ///
    /// Quando a troca acontece, o laço passa a ler os contadores do aparelho
    /// que voltou — como em produção, onde ele larga o `AudioIo` inteiro.
    fn uma_volta(&mut self, agora_ms: f64) {
        let [pendentes, capturadas, em_48k, para_o_aparelho] = &mut self.restos;
        let passo = seguir_o_aparelho(
            &mut self.acompanhamento,
            self.contadores.aviso_de_aparelho(),
            agora_ms,
            &mut self.maquina,
            &self.painel,
            [pendentes, capturadas, em_48k, para_o_aparelho],
            &mut self.relogio,
            self.comecou + Duration::from_micros((agora_ms * 1000.0) as u64),
        );
        match passo {
            PassoDoAparelho::Segue => {}
            PassoDoAparelho::Reaberto(troca) => {
                self.contadores = troca.aberto.contadores;
            }
            // Em produção esta é a volta em que o laço encerra.
            PassoDoAparelho::SemLacoPossivel => self.sem_laco_possivel = true,
        }
    }

    /// Voltas até o relógio andar `quanto_ms`, como o laço anda de 2 em 2 ms.
    ///
    /// Para quando não há mais laço possível, porque é o que o laço faz.
    fn voltas_por(&mut self, desde_ms: f64, quanto_ms: f64) -> f64 {
        let mut agora = desde_ms;
        let fim = desde_ms + quanto_ms;
        while agora < fim && !self.sem_laco_possivel {
            self.uma_volta(agora);
            agora += 2.0;
        }
        agora
    }

    fn painel(&self) -> EstadoDoAudio {
        self.painel
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner())
            .clone()
    }
}

#[test]
fn trocar_o_aparelho_padrao_no_sistema_reabre_a_voz_no_novo() {
    // O caso do relato: a pessoa troca o fone pela bandeja do sistema, sem
    // tocar na tela do SEELE. No Windows o `cpal` manda `DeviceChanged` e
    // **continua tocando no aparelho antigo**.
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");

    let antes = sessao.painel();
    assert_eq!(antes.playback.map(|s| s.name), Some("fone-usb".to_owned()));
    assert_eq!(antes.estado, EstadoDoAparelho::Funcionando);

    // A pessoa escolhe as caixas da mesa como padrão do sistema.
    sessao.maquina.padrao = Some("caixas-da-mesa");
    sessao.o_cpal_avisa(cpal::ErrorKind::DeviceChanged);

    sessao.voltas_por(0.0, 500.0);

    let depois = sessao.painel();
    assert_eq!(
        depois.playback.map(|saida| saida.name),
        Some("caixas-da-mesa".to_owned()),
        "a voz continuou presa ao aparelho antigo; era isto que só reiniciar \
         o aplicativo resolvia"
    );
    assert_eq!(
        depois.capture.map(|microfone| microfone.name),
        Some("caixas-da-mesa".to_owned()),
        "a entrada não seguiu a troca; ela sofre do mesmo defeito que a saída"
    );
    assert_eq!(depois.estado, EstadoDoAparelho::Funcionando);
    assert_eq!(
        depois.reaberturas, 1,
        "a sessão tem de contar a troca, ou a interface não tem como dizer \
         que o aparelho mudou"
    );
    assert_eq!(
        depois.taxas,
        DeviceRates {
            capture_hz: 48_000,
            playback_hz: 48_000,
        },
        "as taxas ficaram nas do aparelho de antes; quem as lê passa a \
         responder um número que já não é de aparelho nenhum"
    );
}

#[test]
fn tirar_o_fone_da_tomada_leva_a_voz_para_o_aparelho_que_sobrou() {
    // No macOS a entrada — e qualquer aparelho aberto por id — vai para o
    // `DisconnectManager`, que **pausa** o fluxo e devolve `DeviceNotAvailable`
    // sem nunca despausar. Sem reabrir, é silêncio até reiniciar.
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");

    // O fone sai da tomada. Por um instante a máquina não oferece nada: é o
    // tempo que o sistema leva para eleger outro padrão.
    sessao.maquina.padrao = None;
    sessao.o_cpal_avisa(cpal::ErrorKind::DeviceNotAvailable);

    let agora = sessao.voltas_por(0.0, 300.0);
    assert!(
        sessao.maquina.aberturas >= 1,
        "ninguém sequer tentou reabrir"
    );
    assert_eq!(
        sessao.painel().estado,
        EstadoDoAparelho::Trocando,
        "a interface tem de saber que o aparelho caiu; o aviso de falha local \
         que existia apagava sozinho e deixava a pessoa no escuro"
    );

    // O sistema elege o alto-falante do laptop.
    sessao.maquina.padrao = Some("alto-falante-do-laptop");
    sessao.voltas_por(agora, 3_000.0);

    let painel = sessao.painel();
    assert_eq!(
        painel.playback.map(|saida| saida.name),
        Some("alto-falante-do-laptop".to_owned()),
        "a voz não seguiu para o aparelho que sobrou"
    );
    assert_eq!(painel.estado, EstadoDoAparelho::Funcionando);
    assert_eq!(painel.reaberturas, 1);
}

#[test]
fn um_estalo_no_fluxo_nao_troca_o_aparelho_de_ninguem() {
    // A metade que protege contra o conserto exagerado. Um `Xrun` é um estalo
    // de máquina carregada, não um aparelho que foi embora — reabrir por causa
    // dele trocaria um clique por um segundo de silêncio, várias vezes ao dia.
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");

    for _ in 0..200 {
        sessao.o_cpal_avisa(cpal::ErrorKind::Xrun);
    }
    sessao.voltas_por(0.0, 5_000.0);

    let painel = sessao.painel();
    assert_eq!(
        sessao.maquina.aberturas, 0,
        "reabriu por causa de um estalo"
    );
    assert_eq!(
        painel.playback.map(|saida| saida.name),
        Some("fone-usb".to_owned())
    );
    assert_eq!(painel.estado, EstadoDoAparelho::Funcionando);
    assert_eq!(painel.reaberturas, 0);
    // Continua sendo contado como tropeço da máquina: nada deixou de ser visto.
    assert_eq!(sessao.contadores.snapshot().stream_errors, 200);
}

#[test]
fn um_aparelho_que_nunca_volta_acaba_dito_perdido() {
    // Tentar para sempre é como um processo fica girando no fundo do laptop de
    // alguém. Quando as tentativas acabam, a sessão tem de **dizer** que não há
    // áudio, e não continuar desenhando o nome de um aparelho que não existe.
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");
    sessao.maquina.padrao = None;
    sessao.o_cpal_avisa(cpal::ErrorKind::DeviceNotAvailable);

    sessao.voltas_por(0.0, 30_000.0);

    assert_eq!(
        sessao.painel().estado,
        EstadoDoAparelho::Perdido,
        "a sessão nunca desistiu, e um estado de «trocando» eterno é a mesma \
         mentira do silêncio calado"
    );
}

#[test]
fn o_fone_religado_depois_de_a_sessao_desistir_volta_a_ter_som() {
    // Desistir não pode ser um beco. A ligação segue de pé enquanto a pessoa
    // procura o cabo; quando ela o acha, meio minuto depois, o som tem de
    // voltar sozinho. Sem a ronda lenta, o único jeito de sair de «perdido»
    // era fechar e abrir o aplicativo — que é o defeito desta tarefa inteira,
    // só que adiado para depois da oitava tentativa.
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");
    sessao.maquina.padrao = None;
    sessao.o_cpal_avisa(cpal::ErrorKind::DeviceNotAvailable);

    let agora = sessao.voltas_por(0.0, 30_000.0);
    assert_eq!(sessao.painel().estado, EstadoDoAparelho::Perdido);
    let tentativas_ate_desistir = sessao.maquina.aberturas;

    // A pessoa acha o cabo e religa o fone. Ninguém toca na tela do SEELE.
    sessao.maquina.padrao = Some("fone-usb");
    sessao.voltas_por(agora, 30_000.0);

    let painel = sessao.painel();
    assert_eq!(
        painel.estado,
        EstadoDoAparelho::Funcionando,
        "o fone voltou à tomada e a sessão continuou dizendo que não há aparelho: \
         só reiniciar o aplicativo resolveria"
    );
    assert_eq!(
        painel.capture.as_ref().map(|d| d.name.as_str()),
        Some("fone-usb")
    );
    assert_eq!(painel.reaberturas, 1, "voltar a ter som é uma reabertura");
    assert!(
        sessao.maquina.aberturas > tentativas_ate_desistir,
        "ninguém foi olhar de novo depois de desistir"
    );
}

#[test]
fn trocar_de_aparelho_duas_vezes_na_mesma_sessao_e_seguido_das_duas_vezes() {
    // Quem troca o fone uma vez troca de volta — sair da reunião e voltar para
    // ela é a mesma pessoa mexendo na bandeja duas vezes. Cada reabertura traz
    // contadores novos e zerados; um ciclo que guardasse o número alto do
    // aparelho anterior deixaria a segunda troca passar em branco, e a pessoa
    // ficaria presa ao aparelho da primeira — o mesmo defeito desta tarefa,
    // adiado para a segunda vez.
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");
    let mut agora = 0.0;

    for aparelho in ["caixas-da-mesa", "fone-usb", "fone-bluetooth"] {
        sessao.maquina.padrao = Some(aparelho);
        sessao.o_cpal_avisa(cpal::ErrorKind::DeviceChanged);
        agora = sessao.voltas_por(agora, 1_000.0);

        let painel = sessao.painel();
        assert_eq!(
            painel.playback.map(|saida| saida.name),
            Some(aparelho.to_owned()),
            "a troca para «{aparelho}» não foi seguida; a voz ficou no aparelho anterior"
        );
        assert_eq!(painel.estado, EstadoDoAparelho::Funcionando);
    }

    assert_eq!(
        sessao.painel().reaberturas,
        3,
        "a interface tem de contar cada troca, e não só a primeira da sessão"
    );
}

#[test]
fn um_panico_noutra_parte_do_programa_nao_congela_a_tela_no_aparelho_antigo() {
    // O cadeado do painel é envenenado para sempre por um pânico que aconteça
    // com ele na mão — e um pânico numa thread não derruba o programa, ele
    // segue tocando. Se a escrita no painel desistisse calada nesse caso, a
    // pessoa veria o aparelho de antes na tela para o resto da sessão, ouvindo
    // pelo de agora: a forma pequena de «o produto sabe e não conta».
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");

    // O hook de pânico é do processo inteiro, não deste teste: calá-lo de
    // vez apagaria a mensagem de qualquer outro teste do mesmo binário que
    // falhasse enquanto este corre. Por isso o pânico proposital acontece numa
    // thread com nome próprio e o hook cala **só** ela, repassando todo o
    // resto a quem já estava lá.
    const THREAD_DO_PANICO: &str = "panico-proposital-do-painel";
    let anterior = std::sync::Arc::new(std::panic::take_hook());
    let repassa = std::sync::Arc::clone(&anterior);
    std::panic::set_hook(Box::new(move |aviso| {
        if std::thread::current().name() == Some(THREAD_DO_PANICO) {
            return;
        }
        repassa(aviso);
    }));

    let painel = &sessao.painel;
    let envenenou = std::thread::scope(|escopo| {
        let subiu = std::thread::Builder::new()
            .name(THREAD_DO_PANICO.to_owned())
            .spawn_scoped(escopo, move || {
                let com_o_painel_na_mao = painel.lock().ok();
                assert!(com_o_painel_na_mao.is_some(), "o painel começa são");
                panic!("um pânico qualquer noutra parte do programa, com o painel na mão");
            });
        match subiu {
            Ok(thread) => thread.join(),
            Err(erro) => panic!("a thread do pânico proposital não subiu: {erro}"),
        }
    });

    // Devolve o comportamento de antes: o `Box` original não sai do `Arc`, mas
    // o hook restaurado chama exatamente ele, sem o filtro.
    std::panic::set_hook(Box::new(move |aviso| anterior(aviso)));
    assert!(
        envenenou.is_err(),
        "o pânico deste teste tinha de acontecer"
    );
    assert!(
        sessao.painel.lock().is_err(),
        "o cadeado tinha de ficar envenenado; sem isso este guarda não prova nada"
    );

    sessao.maquina.padrao = Some("caixas-da-mesa");
    sessao.o_cpal_avisa(cpal::ErrorKind::DeviceChanged);
    sessao.voltas_por(0.0, 500.0);

    let depois = sessao.painel();
    assert_eq!(
        depois.playback.map(|saida| saida.name),
        Some("caixas-da-mesa".to_owned()),
        "a tela ficou congelada no aparelho anterior porque o cadeado estava \
         envenenado; a voz saiu pelo aparelho novo e a pessoa leu o nome errado"
    );
    assert_eq!(
        depois.estado,
        EstadoDoAparelho::Funcionando,
        "o estado do aparelho parou no que a tela via antes do pânico"
    );
}

#[test]
fn a_troca_nao_toca_no_aparelho_novo_o_que_era_do_antigo() {
    // O primeiro som do aparelho novo é o que a pessoa julga. As quatro filas
    // do laço estão na taxa do aparelho de antes; tocadas no de agora, saem
    // como um estalo ou um trecho acelerado — e o reamostrador de antes com o
    // anel de agora sai como «a voz ficou estranha depois que troquei o fone».
    //
    // Este cenário existe porque a ordem entre reabrir, redimensionar e
    // esvaziar é o que se perde numa refatoração sem nada quebrar: cada passo
    // tinha o seu teste de unidade e a composição não tinha nenhum.
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");
    sessao.restos = [
        vec![0.5_f32; 480],
        vec![0.25_f32; 480],
        vec![0.1_f32; 960],
        vec![0.3_f32; 441],
    ];

    // De 44,1 kHz para 48 kHz: a troca que muda taxa, anel e reamostradores.
    sessao.maquina.padrao = Some("caixas-da-mesa");
    sessao.o_cpal_avisa(cpal::ErrorKind::DeviceChanged);
    sessao.voltas_por(0.0, 500.0);

    let painel = sessao.painel();
    assert_eq!(
        painel.playback.map(|saida| saida.name),
        Some("caixas-da-mesa".to_owned())
    );
    assert_eq!(
        painel.taxas,
        DeviceRates {
            capture_hz: 48_000,
            playback_hz: 48_000,
        }
    );
    assert!(
        sessao.restos.iter().all(Vec::is_empty),
        "amostras do aparelho de antes ficaram a caminho do de agora; elas \
         saem como um estalo no primeiro instante do aparelho novo"
    );
    assert!(
        !sessao.sem_laco_possivel,
        "havia laço possível com este aparelho"
    );
}

#[test]
fn um_aparelho_que_abre_e_nao_da_laco_nao_e_desenhado_como_funcionando() {
    // O caminho estreito: o aparelho abre, e o reamostrador recusa as taxas
    // dele. Era o único em que o painel já tinha sido escrito como
    // «funcionando», com o nome do aparelho novo, e o laço encerrava depois —
    // a pessoa ficava sem som nenhum lendo normalidade na tela. É a forma
    // pequena de «o produto sabe e não conta».
    let mut sessao = SessaoDeVoz::comecou_em("fone-usb");

    sessao.maquina.padrao = Some("aparelho-de-taxa-impossivel");
    sessao.o_cpal_avisa(cpal::ErrorKind::DeviceChanged);
    sessao.voltas_por(0.0, 500.0);

    assert!(
        sessao.sem_laco_possivel,
        "o laço seguiu com um aparelho cujas taxas o reamostrador recusa"
    );
    assert_eq!(
        sessao.painel().estado,
        EstadoDoAparelho::Perdido,
        "a tela diz «funcionando» sobre um aparelho que não produz som nenhum"
    );
}
