//! A recepção da sala pessoal: a campainha viva.
//!
//! A [`crate::campainha`] só decide, sem relógio e sem rede. Isto é o que a põe
//! para trabalhar dentro de um servidor, entre três lados (spec de 23/09, «A
//! campainha», «Empurrão», e o ADR 0059):
//!
//! - **as sessões que tocam.** Quem provou a chave pergunta [`Recepcao::chegou`]
//!   e recebe um canal: a sessão segura a conexão até a decisão chegar por ele,
//!   e manda a razão do [`Desfecho`] pelo fio (o aperto em três fases);
//! - **o app de quem recebe.** Ele assina os [`Aviso`]s numa tarefa Rust, e
//!   decide por [`Recepcao::atender`], [`Recepcao::recusar`],
//!   [`Recepcao::desligar`] e [`Recepcao::definir_bloqueados`];
//! - **o barramento do servidor.** Desligar manda `ChamadaEncerrada` a quem está
//!   dentro pelo mesmo [`crate::server::Event::ServidorEncerrando`] com que quem
//!   hospeda fecha avisando.
//!
//! # O log
//!
//! Cada transição vai ao log com o **prefixo** do código, os 4 primeiros
//! símbolos. O código inteiro, nunca: o `seele.log` vai no COPIAR DIAGNÓSTICO, e
//! um código inteiro ali seria o número de telefone de quem ligou na mão de quem
//! lê o diagnóstico.
//!
//! **Os contadores moram no log** (tarefa 31 da F7, decidido em 09/10): cada
//! toque que para diz o desfecho numa palavra, e a sala que fecha diz quantos
//! tocaram, como acabaram e quantos foram recusados sem tocar
//! ([`Recepcao::resumir`]). É onde se lê hoje; o COPIAR DIAGNÓSTICO, quando
//! existir, lê os mesmos números.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use seele_proto::codigo::Codigo;
use seele_proto::control::DisconnectReason;
use tokio::sync::{broadcast, oneshot};

use crate::campainha::{Campainha, Chave, Chegada, Desfecho, IdDoToque, ParouDeTocar};
use crate::server::Event;

/// Quantos avisos o canal do app segura antes de o assinante mais lento perder
/// o mais velho. Um toque são dois avisos; sessenta e quatro é folga de sobra
/// para três pendentes.
const AVISOS_NO_CANAL: usize = 64;

/// O que faz de um servidor a sala pessoal de alguém.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sala {
    /// A chave de quem recebe: ela entra sem tocar, e conta no teto.
    pub dono: Chave,
    /// Quanto tempo cada toque fica tocando. O padrão é
    /// [`crate::campainha::PRAZO_DO_TOQUE`]; um prazo curto é para as provas.
    pub prazo_do_toque: std::time::Duration,
}

impl Sala {
    /// A sala de `dono`, com o prazo de sempre.
    #[must_use]
    pub fn de(dono: Chave) -> Self {
        Self {
            dono,
            prazo_do_toque: crate::campainha::PRAZO_DO_TOQUE,
        }
    }
}

/// O que o app de quem recebe fica sabendo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aviso {
    /// Alguém está tocando. A mesma pessoa tocando de novo (outro candidato da
    /// corrida) chega como outro `Tocou`, com a mesma impressão e outro toque:
    /// a faixa troca o toque, e não soma uma pessoa.
    Tocou {
        /// O toque, para atender ou recusar.
        toque: IdDoToque,
        /// A impressão da chave de quem liga, como a portaria a grava.
        impressao: String,
        /// O código de quem liga: a prova.
        codigo: Codigo,
        /// O apelido que veio no `Hello`: o que a pessoa diz.
        apelido: String,
    },
    /// Um toque parou, e por quê.
    ParouDeTocar {
        /// Qual toque.
        toque: IdDoToque,
        /// A impressão de quem ligava.
        impressao: String,
        /// Como acabou.
        desfecho: Desfecho,
    },
}

/// O que a sessão faz com quem acabou de provar a chave.
#[derive(Debug)]
pub enum Entrada {
    /// Segue para a conta e a sessão, sem tocar.
    Entra,
    /// Manda esta razão e fecha, sem tocar.
    Recusada(DisconnectReason),
    /// Manda `Tocando` e espera a decisão. Se o canal fechar sem decisão, outra
    /// conexão da mesma pessoa tomou o lugar desta, e ela fecha calada.
    Toca {
        /// O toque desta conexão, para [`Recepcao::desistiu`].
        toque: IdDoToque,
        /// Por onde a decisão chega.
        decisao: oneshot::Receiver<Desfecho>,
    },
}

#[derive(Debug)]
struct Estado {
    campainha: Campainha,
    esperando: HashMap<IdDoToque, oneshot::Sender<Desfecho>>,
    contagem: Contagem,
}

/// O que a campainha desta sala viu desde que subiu, para o resumo do log.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Contagem {
    /// Toques que começaram, contando a mesma pessoa de novo por outro
    /// candidato só uma vez.
    tocaram: u32,
    atendidas: u32,
    recusadas: u32,
    ninguem_atendeu: u32,
    cairam_antes: u32,
    cheias: u32,
    /// Recusados sem tocar, por excesso: recusou há pouco, ou já há três
    /// tocando.
    recusados_por_taxa: u32,
    /// Recusados sem tocar, por estarem bloqueados na agenda.
    bloqueados: u32,
    /// Recusados sem tocar, com a ligação cheia.
    cheia_sem_tocar: u32,
}

impl Contagem {
    fn parou(&mut self, desfecho: Desfecho) {
        let contador = match desfecho {
            Desfecho::Atendida => &mut self.atendidas,
            Desfecho::Recusada => &mut self.recusadas,
            Desfecho::NaoAtendida => &mut self.ninguem_atendeu,
            Desfecho::Cancelada => &mut self.cairam_antes,
            Desfecho::Cheia => &mut self.cheias,
        };
        *contador = contador.saturating_add(1);
    }

    fn recusou_sem_tocar(&mut self, recusa: crate::campainha::Recusa) {
        use crate::campainha::Recusa;
        let contador = match recusa {
            Recusa::AcabouDeRecusar | Recusa::MuitosTocando => &mut self.recusados_por_taxa,
            Recusa::Bloqueado => &mut self.bloqueados,
            Recusa::Cheia => &mut self.cheia_sem_tocar,
        };
        *contador = contador.saturating_add(1);
    }
}

/// O desfecho de um toque numa palavra só, a que o log escreve.
///
/// Estável, e não o `Debug` do tipo: quem procura no `seele.log` os toques que
/// ninguém atendeu procura uma palavra, e ela não pode mudar com o nome de uma
/// variante.
fn palavra(desfecho: Desfecho) -> &'static str {
    match desfecho {
        Desfecho::Atendida => "atendida",
        Desfecho::Recusada => "recusada",
        Desfecho::NaoAtendida => "ninguem_atendeu",
        Desfecho::Cancelada => "caiu_antes",
        Desfecho::Cheia => "cheia",
    }
}

/// A campainha de uma sala, viva.
#[derive(Debug)]
pub struct Recepcao {
    estado: Mutex<Estado>,
    avisos: broadcast::Sender<Aviso>,
    eventos: broadcast::Sender<Event>,
    sala: Sala,
}

/// Os 4 primeiros símbolos do código, para o log.
fn prefixo(codigo: &Codigo) -> String {
    codigo.to_string().chars().take(4).collect()
}

impl Recepcao {
    /// A recepção da sala de `dono`, ligada ao barramento `eventos` do servidor.
    #[must_use]
    pub fn nova(dono: Chave, eventos: broadcast::Sender<Event>) -> Self {
        Self::da_sala(Sala::de(dono), eventos)
    }

    /// A recepção desta sala, ligada ao barramento `eventos` do servidor.
    #[must_use]
    pub fn da_sala(sala: Sala, eventos: broadcast::Sender<Event>) -> Self {
        let (avisos, _) = broadcast::channel(AVISOS_NO_CANAL);
        Self {
            estado: Mutex::new(Estado {
                campainha: Campainha::nova(sala.dono).com_prazo(sala.prazo_do_toque),
                esperando: HashMap::new(),
                contagem: Contagem::default(),
            }),
            avisos,
            eventos,
            sala,
        }
    }

    /// A sala desta recepção.
    #[must_use]
    pub fn sala(&self) -> Sala {
        self.sala
    }

    /// Um canal com os avisos daqui em diante.
    #[must_use]
    pub fn assinar(&self) -> broadcast::Receiver<Aviso> {
        self.avisos.subscribe()
    }

    /// Alguém provou a chave e quer entrar.
    pub fn chegou(&self, chave: Chave, apelido: &str) -> Entrada {
        self.chegou_em(chave, apelido, Instant::now())
    }

    /// [`Self::chegou`], com o instante dado.
    pub fn chegou_em(&self, chave: Chave, apelido: &str, agora: Instant) -> Entrada {
        let mut estado = self.estado();
        let codigo = Codigo::da_chave(&chave);
        match estado.campainha.chegou(chave, apelido, agora) {
            Chegada::Entra => Entrada::Entra,
            Chegada::Recusada(recusa) => {
                estado.contagem.recusou_sem_tocar(recusa);
                tracing::info!(
                    codigo = %prefixo(&codigo),
                    ?recusa,
                    "campainha: recusada sem tocar"
                );
                Entrada::Recusada(recusa.razao())
            }
            Chegada::Toca { toque, substituiu } => {
                // A conexão substituída perde o canal, e fecha calada: a mesma
                // pessoa continua tocando por esta.
                if let Some(velho) = substituiu {
                    estado.esperando.remove(&velho);
                } else {
                    estado.contagem.tocaram = estado.contagem.tocaram.saturating_add(1);
                }
                let (envia, decisao) = oneshot::channel();
                estado.esperando.insert(toque, envia);
                drop(estado);
                tracing::info!(
                    codigo = %prefixo(&codigo),
                    ?toque,
                    substituiu = substituiu.is_some(),
                    "campainha: tocando"
                );
                let _ = self.avisos.send(Aviso::Tocou {
                    toque,
                    impressao: seele_proto::transport::key_fingerprint(&chave),
                    codigo,
                    apelido: apelido.to_owned(),
                });
                Entrada::Toca { toque, decisao }
            }
        }
    }

    /// Quem recebe atendeu. `false` se o toque já tinha acabado.
    pub fn atender(&self, toque: IdDoToque) -> bool {
        let parou = self.estado().campainha.atender(toque);
        self.anunciar(parou.into_iter().collect())
    }

    /// Quem recebe recusou. `false` se o toque já tinha acabado.
    pub fn recusar(&self, toque: IdDoToque) -> bool {
        let parou = self.estado().campainha.recusar(toque, Instant::now());
        self.anunciar(parou.into_iter().collect())
    }

    /// A conexão que tocava fechou.
    pub fn desistiu(&self, toque: IdDoToque) {
        let parou = self.estado().campainha.desistiu(toque);
        self.anunciar(parou.into_iter().collect());
    }

    /// Para como «não atendeu» todo toque cujo prazo venceu.
    pub fn vencer(&self) {
        self.vencer_em(Instant::now());
    }

    /// [`Self::vencer`], com o instante dado.
    pub fn vencer_em(&self, agora: Instant) {
        let parados = self.estado().campainha.vencer(agora);
        self.anunciar(parados);
    }

    /// Alguém que estava na ligação saiu.
    pub fn saiu(&self, chave: Chave) {
        self.estado().campainha.saiu(chave, Instant::now());
        tracing::info!(
            codigo = %prefixo(&Codigo::da_chave(&chave)),
            "campainha: saiu da ligação"
        );
    }

    /// Troca a lista de bloqueados, ao vivo. Quem estava tocando e passou a
    /// estar bloqueado para de tocar como recusado.
    pub fn definir_bloqueados(&self, bloqueados: Vec<Codigo>) {
        let parados = self.estado().campainha.definir_bloqueados(bloqueados);
        self.anunciar(parados);
    }

    /// Quem recebe desligou: quem estava dentro recebe `ChamadaEncerrada` pelo
    /// barramento, e as aprovações se apagam. Devolve quem estava dentro.
    pub fn desligar(&self) -> Vec<Chave> {
        let dentro = self.estado().campainha.desligar();
        tracing::info!(convidados = dentro.len(), "campainha: quem recebe desligou");
        // Sem ninguém dentro, não há a quem avisar, e um fim mandado a ninguém
        // seria uma linha a mais no log de cada sessão que não é da ligação.
        if !dentro.is_empty() {
            let _ = self.eventos.send(Event::ServidorEncerrando {
                reason: DisconnectReason::ChamadaEncerrada,
            });
        }
        dentro
    }

    /// A linha de resumo da sala que fecha: quantos tocaram, como acabaram, e
    /// quantos foram recusados sem tocar. Sem identidade nenhuma: os números
    /// são da sala, e quem tocou já está nas linhas de cada toque, pelo
    /// prefixo.
    ///
    /// Quem chama é a hospedagem, ao fechar a sala (`Hospedagem::encerrar` e
    /// `Hospedagem::despedir`).
    pub fn resumir(&self) {
        let contagem = self.estado().contagem;
        tracing::info!(
            tocaram = contagem.tocaram,
            atendidas = contagem.atendidas,
            recusadas = contagem.recusadas,
            ninguem_atendeu = contagem.ninguem_atendeu,
            cairam_antes = contagem.cairam_antes,
            cheias = contagem.cheias,
            recusados_por_taxa = contagem.recusados_por_taxa,
            bloqueados = contagem.bloqueados,
            cheia_sem_tocar = contagem.cheia_sem_tocar,
            "campainha: a sala fechou"
        );
    }

    fn estado(&self) -> std::sync::MutexGuard<'_, Estado> {
        // Um pânico com a trava na mão não deixa a campainha num estado pela
        // metade: cada método muda a campainha numa chamada só.
        self.estado.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Entrega cada desfecho à sessão que espera por ele, escreve a linha do
    /// log e avisa o app. Devolve se havia algum.
    fn anunciar(&self, parados: Vec<ParouDeTocar>) -> bool {
        let algum = !parados.is_empty();
        for parou in parados {
            {
                let mut estado = self.estado();
                estado.contagem.parou(parou.desfecho);
                if let Some(envia) = estado.esperando.remove(&parou.toque) {
                    let _ = envia.send(parou.desfecho);
                }
            }
            tracing::info!(
                codigo = %prefixo(&Codigo::da_chave(&parou.chave)),
                toque = ?parou.toque,
                desfecho = %palavra(parou.desfecho),
                "campainha: parou de tocar"
            );
            let _ = self.avisos.send(Aviso::ParouDeTocar {
                toque: parou.toque,
                impressao: seele_proto::transport::key_fingerprint(&parou.chave),
                desfecho: parou.desfecho,
            });
        }
        algum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campainha::PRAZO_DO_TOQUE;

    const DONO: Chave = [1; 32];

    fn chave(n: u8) -> Chave {
        [n; 32]
    }

    fn recepcao() -> (Recepcao, broadcast::Receiver<Event>) {
        let (eventos, ouvindo) = broadcast::channel(16);
        (Recepcao::nova(DONO, eventos), ouvindo)
    }

    fn toca(entrada: Entrada) -> (IdDoToque, oneshot::Receiver<Desfecho>) {
        match entrada {
            Entrada::Toca { toque, decisao } => (toque, decisao),
            outra => panic!("devia tocar, e veio {outra:?}"),
        }
    }

    #[tokio::test]
    async fn tocar_avisa_o_app_e_atender_libera_a_sessao() {
        let (recepcao, _) = recepcao();
        let mut avisos = recepcao.assinar();
        let (toque, decisao) = toca(recepcao.chegou(chave(2), "rafa"));

        assert_eq!(
            avisos.try_recv().unwrap(),
            Aviso::Tocou {
                toque,
                impressao: seele_proto::transport::key_fingerprint(&chave(2)),
                codigo: Codigo::da_chave(&chave(2)),
                apelido: "rafa".into(),
            },
            "o app não ficou sabendo que tocava"
        );
        assert!(recepcao.atender(toque));
        assert_eq!(
            decisao.await.unwrap(),
            Desfecho::Atendida,
            "a sessão que tocava não soube que foi atendida"
        );
        assert!(matches!(
            avisos.try_recv().unwrap(),
            Aviso::ParouDeTocar {
                desfecho: Desfecho::Atendida,
                ..
            }
        ));
        assert!(
            !recepcao.atender(toque),
            "o mesmo toque foi atendido duas vezes"
        );
    }

    #[tokio::test]
    async fn recusar_manda_a_sessao_embora_com_a_razao_de_recusa() {
        let (recepcao, _) = recepcao();
        let (toque, decisao) = toca(recepcao.chegou(chave(2), "rafa"));
        assert!(recepcao.recusar(toque));
        let desfecho = decisao.await.unwrap();
        assert_eq!(desfecho, Desfecho::Recusada);
        assert_eq!(desfecho.razao(), Some(DisconnectReason::ChamadaRecusada));
    }

    #[test]
    fn quem_desiste_para_de_tocar_no_app() {
        let (recepcao, _) = recepcao();
        let mut avisos = recepcao.assinar();
        let (toque, _decisao) = toca(recepcao.chegou(chave(2), "rafa"));
        let _ = avisos.try_recv();
        recepcao.desistiu(toque);
        assert!(
            matches!(
                avisos.try_recv().unwrap(),
                Aviso::ParouDeTocar {
                    desfecho: Desfecho::Cancelada,
                    ..
                }
            ),
            "a faixa continuaria tocando para quem já desligou"
        );
    }

    #[tokio::test]
    async fn o_prazo_vencido_chega_a_sessao_e_ao_app() {
        let (recepcao, _) = recepcao();
        let mut avisos = recepcao.assinar();
        let agora = Instant::now();
        let (_, decisao) = toca(recepcao.chegou_em(chave(2), "rafa", agora));
        let _ = avisos.try_recv();

        recepcao.vencer_em(agora + PRAZO_DO_TOQUE);
        assert_eq!(decisao.await.unwrap(), Desfecho::NaoAtendida);
        assert!(matches!(
            avisos.try_recv().unwrap(),
            Aviso::ParouDeTocar {
                desfecho: Desfecho::NaoAtendida,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn o_candidato_substituido_fecha_calado_e_o_toque_continua() {
        let (recepcao, _) = recepcao();
        let mut avisos = recepcao.assinar();
        let (velho, decisao_velha) = toca(recepcao.chegou(chave(2), "rafa"));
        let (novo, _decisao_nova) = toca(recepcao.chegou(chave(2), "rafa"));

        assert!(
            decisao_velha.await.is_err(),
            "a conexão substituída recebeu uma decisão, e não só o canal fechado"
        );
        let recebidos: Vec<Aviso> = std::iter::from_fn(|| avisos.try_recv().ok()).collect();
        assert_eq!(recebidos.len(), 2, "vieram {recebidos:?}");
        assert!(
            recebidos
                .iter()
                .all(|aviso| matches!(aviso, Aviso::Tocou { .. })),
            "a substituição parou o toque no app: {recebidos:?}"
        );
        assert!(!recepcao.atender(velho));
        assert!(recepcao.atender(novo));
    }

    #[test]
    fn desligar_encerra_quem_esta_dentro_pelo_barramento() {
        let (recepcao, mut barramento) = recepcao();
        assert!(recepcao.desligar().is_empty());
        assert!(
            barramento.try_recv().is_err(),
            "desligar sem ninguém dentro mandou um aviso de fim a ninguém"
        );

        let (toque, _decisao) = toca(recepcao.chegou(chave(2), "rafa"));
        assert!(recepcao.atender(toque));
        assert_eq!(recepcao.desligar(), vec![chave(2)]);
        assert!(
            matches!(
                barramento.try_recv().unwrap(),
                Event::ServidorEncerrando {
                    reason: DisconnectReason::ChamadaEncerrada
                }
            ),
            "quem estava na ligação não recebeu ChamadaEncerrada"
        );
    }

    #[tokio::test]
    async fn bloquear_ao_vivo_recusa_quem_esta_tocando() {
        let (recepcao, _) = recepcao();
        let (_, decisao) = toca(recepcao.chegou(chave(2), "rafa"));
        recepcao.definir_bloqueados(vec![Codigo::da_chave(&chave(2))]);
        assert_eq!(decisao.await.unwrap(), Desfecho::Recusada);
        assert!(matches!(
            recepcao.chegou(chave(2), "rafa"),
            Entrada::Recusada(DisconnectReason::ChamadaRecusada)
        ));
    }

    #[test]
    fn cada_transicao_vai_ao_log_com_o_prefixo_do_codigo_e_nunca_o_codigo_inteiro() {
        use std::io::Write;
        use std::sync::Arc;

        #[derive(Clone, Default)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);
        impl Write for Buffer {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Buffer::default();
        let capturado = buffer.clone();
        let assinante = tracing_subscriber::fmt()
            .with_writer(move || capturado.clone())
            .without_time()
            .with_ansi(false)
            .finish();
        let (recepcao, _) = recepcao();
        // Um segundo despachante vivo: com um só registrado, o `tracing-core`
        // 0.1.36 deixa a thread que passa primeiro por uma linha decidir por
        // todas, e os testes vizinhos sem captura apagavam esta. É o auxiliar
        // do crate, o mesmo `segundo_despachante` do app (`1405210b`). Medido
        // aqui: 3/3 rodadas vermelhas sem ele, 20/20 verdes com ele.
        crate::rastro_de_teste::segundo_despachante();
        tracing::subscriber::with_default(assinante, || {
            let (toque, _decisao) = toca(recepcao.chegou(chave(2), "rafa"));
            recepcao.recusar(toque);
        });

        let log = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        let codigo = Codigo::da_chave(&chave(2));
        assert_eq!(
            log.lines()
                .filter(|linha| linha.contains("campainha"))
                .count(),
            2,
            "o toque e a recusa não deram uma linha cada: {log}"
        );
        assert!(log.contains(&prefixo(&codigo)), "o log não diz quem: {log}");
        assert!(
            !log.contains(&codigo.to_string()) && !log.contains(&codigo.dado()),
            "o log tem o código inteiro de quem ligou: {log}"
        );
    }

    /// O que `fazer` escreveu no log, com o segundo despachante vivo (ver o
    /// teste de cima, e o `rastro_de_teste` do crate).
    fn capturar(fazer: impl FnOnce()) -> String {
        use std::io::Write;
        use std::sync::Arc;

        #[derive(Clone, Default)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);
        impl Write for Buffer {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Buffer::default();
        let capturado = buffer.clone();
        let assinante = tracing_subscriber::fmt()
            .with_writer(move || capturado.clone())
            .without_time()
            .with_ansi(false)
            .finish();
        crate::rastro_de_teste::segundo_despachante();
        tracing::subscriber::with_default(assinante, fazer);
        let log = buffer.0.lock().unwrap().clone();
        String::from_utf8(log).unwrap()
    }

    #[test]
    fn cada_toque_que_para_diz_o_desfecho_numa_palavra() {
        // Tarefa 31: o produto sabe como cada toque acabou, e conta onde se lê
        // hoje. Uma palavra estável, e não o `Debug` do tipo, para quem procura
        // no `seele.log` os toques que ninguém atendeu.
        let (recepcao, _) = recepcao();
        let log = capturar(|| {
            let (atendido, _a) = toca(recepcao.chegou(chave(2), "a"));
            recepcao.atender(atendido);
            let (recusado, _b) = toca(recepcao.chegou(chave(3), "b"));
            recepcao.recusar(recusado);
            let (caiu, _c) = toca(recepcao.chegou(chave(4), "c"));
            recepcao.desistiu(caiu);
            let (_, _d) = toca(recepcao.chegou(chave(5), "d"));
            recepcao.vencer_em(Instant::now() + PRAZO_DO_TOQUE + std::time::Duration::from_secs(1));
        });

        let pararam: Vec<&str> = log
            .lines()
            .filter(|linha| linha.contains("campainha: parou de tocar"))
            .collect();
        assert_eq!(pararam.len(), 4, "{log}");
        for (codigo, palavra) in [
            (chave(2), "desfecho=atendida"),
            (chave(3), "desfecho=recusada"),
            (chave(4), "desfecho=caiu_antes"),
            (chave(5), "desfecho=ninguem_atendeu"),
        ] {
            let quem = prefixo(&Codigo::da_chave(&codigo));
            assert!(
                pararam
                    .iter()
                    .any(|linha| linha.contains(&quem) && linha.contains(palavra)),
                "o toque de {quem} não disse `{palavra}`: {log}"
            );
        }
    }

    #[test]
    fn a_sala_que_fecha_diz_quantos_tocaram_e_como_acabaram() {
        // A linha de resumo, com cada contador, e sem identidade nenhuma.
        let (recepcao, _) = recepcao();
        recepcao.definir_bloqueados(vec![Codigo::da_chave(&chave(7))]);
        let log = capturar(|| {
            let (a, _a) = toca(recepcao.chegou(chave(2), "a"));
            recepcao.atender(a);
            let (b, _b) = toca(recepcao.chegou(chave(3), "b"));
            recepcao.recusar(b);
            // Recusado há pouco: recusado de novo sem tocar, por excesso.
            assert!(matches!(
                recepcao.chegou(chave(3), "b"),
                Entrada::Recusada(_)
            ));
            let (c, _c) = toca(recepcao.chegou(chave(4), "c"));
            recepcao.desistiu(c);
            // A mesma pessoa por dois candidatos é um toque só.
            let (_, _d1) = toca(recepcao.chegou(chave(5), "d"));
            let (_, _d2) = toca(recepcao.chegou(chave(5), "d"));
            recepcao.vencer_em(Instant::now() + PRAZO_DO_TOQUE + std::time::Duration::from_secs(1));
            // Bloqueado: recusado sem tocar.
            assert!(matches!(
                recepcao.chegou(chave(7), "e"),
                Entrada::Recusada(_)
            ));
            recepcao.resumir();
        });

        let resumo: Vec<&str> = log
            .lines()
            .filter(|linha| linha.contains("campainha: a sala fechou"))
            .collect();
        assert_eq!(resumo.len(), 1, "o resumo não saiu uma vez só: {log}");
        let resumo = resumo.first().copied().unwrap_or_default();
        for campo in [
            "tocaram=4",
            "atendidas=1",
            "recusadas=1",
            "ninguem_atendeu=1",
            "cairam_antes=1",
            "cheias=0",
            "recusados_por_taxa=1",
            "bloqueados=1",
            "cheia_sem_tocar=0",
        ] {
            assert!(
                resumo.contains(campo),
                "o resumo não diz `{campo}`: {resumo}"
            );
        }
        for chave in [2, 3, 4, 5, 7].map(chave) {
            assert!(
                !resumo.contains(&prefixo(&Codigo::da_chave(&chave))),
                "o resumo diz quem tocou, e ele é da sala: {resumo}"
            );
        }
    }
}
