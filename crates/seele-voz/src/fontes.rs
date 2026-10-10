//! A tabela de quem fala, do lado de quem recebe: uma fonte por SSRC, criada
//! na primeira chegada, a própria voz jogada fora, e quem calou esquecido.
//!
//! Morava dentro do laço da voz do desktop (`seele-core/src/voice.rs`), e saiu
//! para cá para o navegador usar a mesma tabela (Tarefa 3.3 da F4; o ADR 0057
//! proíbe uma segunda implementação). O relógio entra por argumento, em
//! milissegundos: é o que deixa testar o esquecer sem esperar 30 s (o teste
//! gravado está no `seele-core`, `voice::fontes_gravadas`).
//!
//! **O que cada fonte guarda é de quem a usa** (`T`): o desktop guarda o buffer
//! de jitter, a deriva e o decodificador; o navegador, a alça opaca que a
//! página conhece no lugar do SSRC.

/// Quanto silêncio faz uma pessoa ser esquecida por quem escuta.
///
/// # Por que esquecer alguém é bom
///
/// A tabela de quem fala era um `Vec` que só crescia: uma pessoa que entrou,
/// falou e saiu ficava lá com um decodificador e um buffer inteiros até a
/// sessão acabar. Isso era só desperdício.
///
/// O que **não** era desperdício é a segunda metade: um buffer velho guarda até
/// onde aquela pessoa já tocou, e recusa qualquer carimbo anterior a isso. Se o
/// relógio dela recomeçar por qualquer motivo — o salto do
/// [`crate::relogio::RelogioDeMidia`] cobre o motivo conhecido, e nada garante que era o
/// único —, a pessoa some para sempre e nem sair e voltar resolve. Esquecê-la
/// depois de meio minuto calado transforma «para sempre» em «meio minuto».
///
/// Meio minuto e não cinco segundos porque o silêncio aqui é normal: o DTX não
/// manda nada enquanto ninguém fala, então uma pessoa quieta numa reunião fica
/// sem mandar um quadro por minutos a fio. O preço de esquecer é o buffer
/// encher de novo na primeira palavra — algumas dezenas de milissegundos.
pub const SILENCIO_ATE_ESQUECER_MS: f64 = 30_000.0;

/// Uma fonte da tabela.
#[derive(Debug)]
struct Fonte<T> {
    ssrc: u32,
    /// Quando chegou o último quadro desta pessoa, no relógio de quem chama.
    ultimo_ms: f64,
    estado: T,
}

/// Quem está falando, na ordem em que chegou, sem a própria voz.
#[derive(Debug)]
pub struct TabelaDeFontes<T> {
    minha: u32,
    fontes: Vec<Fonte<T>>,
}

impl<T> TabelaDeFontes<T> {
    /// Uma tabela vazia para quem fala com o SSRC `minha`, que nunca entra.
    #[must_use]
    pub const fn nova(minha: u32) -> Self {
        Self {
            minha,
            fontes: Vec::new(),
        }
    }

    /// Um quadro de `ssrc` chegou em `agora_ms`. Devolve a fonte dele, criada
    /// por `criar` se é a primeira chegada; `None` para a própria voz, e para a
    /// fonte nova que `criar` não conseguiu montar (um decodificador que não
    /// abre não deixa fonte nenhuma na tabela).
    pub fn chegou(
        &mut self,
        ssrc: u32,
        agora_ms: f64,
        criar: impl FnOnce() -> Option<T>,
    ) -> Option<&mut T> {
        if ssrc == self.minha {
            return None;
        }
        let indice = match self.fontes.iter().position(|fonte| fonte.ssrc == ssrc) {
            Some(indice) => indice,
            None => {
                self.fontes.push(Fonte {
                    ssrc,
                    ultimo_ms: agora_ms,
                    estado: criar()?,
                });
                self.fontes.len() - 1
            }
        };
        let fonte = self.fontes.get_mut(indice)?;
        fonte.ultimo_ms = agora_ms;
        Some(&mut fonte.estado)
    }

    /// Esquece quem calou [`SILENCIO_ATE_ESQUECER_MS`] ou mais até `agora_ms`.
    pub fn esquecer_caladas(&mut self, agora_ms: f64) {
        self.fontes
            .retain(|fonte| agora_ms - fonte.ultimo_ms < SILENCIO_ATE_ESQUECER_MS);
    }

    /// Cada fonte, com o SSRC dela, na ordem em que chegou.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &T)> {
        self.fontes.iter().map(|fonte| (fonte.ssrc, &fonte.estado))
    }

    /// Cada fonte, com o SSRC dela, para mexer no que ela guarda.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u32, &mut T)> {
        self.fontes
            .iter_mut()
            .map(|fonte| (fonte.ssrc, &mut fonte.estado))
    }

    /// Quantas fontes há.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fontes.len()
    }

    /// Se não há fonte nenhuma.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fontes.is_empty()
    }
}
