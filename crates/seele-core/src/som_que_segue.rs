//! O som da tela compartilhada seguindo a troca de aparelho.
//!
//! # O defeito
//!
//! `video::som_da_maquina` abria o *loopback* **uma vez** — `CapturaDaSaida::
//! abrir(None)`, o padrão do sistema naquele instante — e o segurava pelo resto
//! da transmissão. Quem trocasse de fone pela bandeja do Windows no meio de um
//! compartilhamento continuava enviando o som do aparelho antigo: no melhor
//! caso o de um aparelho que ninguém mais ouve, no caso de o aparelho ter sido
//! retirado, silêncio. E silêncio numa transmissão não parece defeito — parece
//! que o jogo estava mudo.
//!
//! O aviso sempre esteve lá. `device::abrir_entrada` instala o mesmo
//! `retorno_de_erro` no fluxo do loopback que instala no da voz, então trocas e
//! sumiços já eram contados. Faltava alguém para lê-los.
//!
//! # Por que este módulo não sabe que existe Windows
//!
//! Porque `video::som_da_maquina` é `#[cfg(target_os = "windows")]`, e um teste
//! escrito lá dentro **nunca roda num Mac** — é o «existir não é funcionar» que
//! o `CLAUDE.md` deste repositório nomeia, e há um caso exato dele no histórico:
//! um teste dentro de um `#![cfg(windows)]` que nunca rodou.
//!
//! A decisão — ler o aviso, perguntar ao ciclo, reabrir, passar a ler do novo —
//! não tem nada de plataforma. Ela mora aqui, genérica sobre quem abre, e a
//! parte que só existe no Windows fica sendo a implementação de
//! [`seele_audio::supervisor::Reabertura`] que chama o `cpal`. O teste de
//! conformidade conduz **esta** função com uma abertura de mentira, e roda em
//! qualquer máquina.
//!
//! # Por que reaproveita o ciclo em vez de repetir a decisão
//!
//! [`CicloDoAparelho`] já é a máquina de estados que a tarefa da voz ligou:
//! quando tentar de novo, quanto esperar entre tentativas, quando desistir e
//! chamar o aparelho de perdido. Escrever uma segunda aqui seria duas máquinas
//! discordando sobre o mesmo aparelho — e uma delas ficaria sem os consertos da
//! outra no dia seguinte.

use std::time::Instant;

use seele_audio::supervisor::{AvisoDeAparelho, CicloDoAparelho, Reabertura};

/// Uma captura de som aberta, no que este módulo precisa dela.
///
/// Duas perguntas e nada mais: dá-me amostras, e diz o que o `cpal` avisou
/// sobre ti. A produção a satisfaz com `seele_audio::laco::CapturaDaSaida`; um
/// teste a satisfaz com um punhado de campos.
pub trait SomAberto {
    /// Tira até `teto` amostras do anel, na taxa da casa.
    fn tomar(&self, teto: usize) -> Vec<f32>;
    /// Trocas e sumiços que o `cpal` contou neste aparelho.
    fn aviso(&self) -> AvisoDeAparelho;
}

/// O som da máquina, seguindo o aparelho que a máquina tem agora.
///
/// Guarda o que está aberto e o ciclo que decide quando trocá-lo. Uma volta é
/// [`Self::tomar`]: ela pergunta ao aparelho aberto o que ele avisou, deixa o
/// ciclo decidir, e só então tira amostras — do aparelho novo, quando houve um.
pub struct SomQueSegue<R: Reabertura> {
    ciclo: CicloDoAparelho,
    /// O que está aberto agora, ou nada enquanto o ciclo procura um aparelho.
    ///
    /// Enquanto o ciclo tenta de novo, o que está aqui é o aparelho **antigo**
    /// — a mesma escolha que o laço da voz faz. Ele quase sempre entrega
    /// silêncio, e silêncio por alguns instantes é melhor que largar o anel e
    /// ficar sem nada a que voltar se a reabertura falhar.
    ///
    /// **O que este módulo não trata**, e é honesto dizer: a *primeira*
    /// abertura falhando continua sendo definitiva. `video::som_da_maquina`
    /// devolve `None` e a transmissão sai muda sem nada tentar de novo. É outro
    /// defeito, de outra família — este aqui é sobre a troca depois de ter
    /// aberto — e fica nomeado em vez de consertado de passagem.
    aberto: Option<R::Aberto>,
    abridor: R,
    comecou: Instant,
}

impl<R> SomQueSegue<R>
where
    R: Reabertura,
    R::Aberto: SomAberto,
{
    /// Começa com o que já está aberto.
    pub fn novo(aberto: R::Aberto, abridor: R) -> Self {
        Self {
            ciclo: CicloDoAparelho::novo(),
            aberto: Some(aberto),
            abridor,
            comecou: Instant::now(),
        }
    }

    /// Uma volta: acompanha o aparelho e devolve o que houver para mandar.
    ///
    /// O relógio sai daqui e entra em [`Self::tomar_em`] pela mesma razão que
    /// `seguir_o_aparelho` recebe o dele de fora: o ciclo espera entre
    /// tentativas, e um teste que tivesse de esperar de verdade ou levaria
    /// segundos por asserção ou mediria o escalonador desta máquina. Produção
    /// chama esta; o teste chama a de baixo com o tempo na mão.
    pub fn tomar(&mut self, teto: usize) -> Vec<f32> {
        let agora_ms = self.comecou.elapsed().as_secs_f64() * 1_000.0;
        self.tomar_em(teto, agora_ms)
    }

    /// A mesma volta, com o relógio dado.
    pub fn tomar_em(&mut self, teto: usize, agora_ms: f64) -> Vec<f32> {
        // O aviso vem do aparelho **aberto agora**. Sem nada aberto, o ciclo já
        // sabe que está procurando e um aviso zerado não o faz recuar.
        let aviso = self
            .aberto
            .as_ref()
            .map_or_else(AvisoDeAparelho::default, SomAberto::aviso);
        if let Some(novo) = self.ciclo.passo(aviso, agora_ms, &mut self.abridor) {
            tracing::info!("o som da transmissão reabriu no aparelho de agora");
            self.aberto = Some(novo);
        }
        self.aberto
            .as_ref()
            .map_or_else(Vec::new, |aberto| aberto.tomar(teto))
    }

    /// Em que pé está o aparelho, para quem quiser contar.
    #[must_use]
    pub fn estado(&self) -> seele_audio::supervisor::DeviceState {
        self.ciclo.estado()
    }
}
