use super::{correr, ConnectError};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const DEFASAGEM: Duration = Duration::from_millis(250);

/// Uma corrida sem ninguém que chegue depois: a de sempre.
fn sem_tardios() -> Option<std::future::Pending<Vec<usize>>> {
    None
}

/// Quem chega `depois`, e com quem.
fn tardios(
    depois: Duration,
    quem: Vec<usize>,
) -> Option<impl std::future::Future<Output = Vec<usize>>> {
    Some(async move {
        tokio::time::sleep(depois).await;
        quem
    })
}

/// Uma tentativa de teste: o que `correr` recebe de `tentar`.
type Futuro =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize, ConnectError>> + Send>>;

/// O diário de quem começou: posição, quem, e em que milissegundo.
type Diario = Arc<std::sync::Mutex<Vec<(usize, usize, u128)>>>;

/// Quem começou, com o quê e quando, e o que cada um faz: os candidatos de
/// `rapidos` fecham em 10 ms, os de `falham` falham na hora, e os outros
/// demoram uma eternidade e então desistem.
fn anotando(
    rapidos: &'static [usize],
    falham: &'static [usize],
) -> (Diario, impl Fn(usize, usize) -> Futuro) {
    let comeco = tokio::time::Instant::now();
    let quando = Arc::new(std::sync::Mutex::new(Vec::new()));
    let anotar = Arc::clone(&quando);
    let tentar = move |posicao: usize, quem: usize| {
        anotar
            .lock()
            .unwrap()
            .push((posicao, quem, comeco.elapsed().as_millis()));
        Box::pin(async move {
            if falham.contains(&quem) {
                return Err(ConnectError::Unreachable);
            }
            if !rapidos.contains(&quem) {
                tokio::time::sleep(Duration::from_secs(100)).await;
                return Err(ConnectError::SemResposta);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(quem)
        }) as Futuro
    };
    (quando, tentar)
}

/// O primeiro a fechar vence, mesmo estando por último na lista.
///
/// É a propriedade inteira do RFC 8305, e a que a série não tem: em série o
/// candidato lento **precisa** terminar antes de o rápido começar.
#[tokio::test(start_paused = true)]
async fn o_rapido_vence_mesmo_estando_por_ultimo() {
    let corrida = correr(
        vec![0, 1, 2],
        DEFASAGEM,
        sem_tardios(),
        None,
        |posicao, _| async move {
            // Os dois primeiros demoram uma eternidade; o terceiro fecha logo.
            let espera = if posicao == 2 { 50 } else { 100_000 };
            tokio::time::sleep(Duration::from_millis(espera)).await;
            Ok::<usize, ConnectError>(posicao)
        },
    )
    .await;

    assert_eq!(corrida.vencedor.map(|(posicao, _)| posicao), Some(2));
}

/// O segundo candidato começa sem esperar o primeiro terminar.
///
/// Sem esta propriedade não há Happy Eyeballs nenhum — é literalmente a
/// diferença entre 9,6 s e ~1,1 s na pendência nº 26.
#[tokio::test(start_paused = true)]
async fn o_segundo_comeca_antes_de_o_primeiro_terminar() {
    let comecaram = Arc::new(AtomicUsize::new(0));
    let contador = Arc::clone(&comecaram);

    let corrida = correr(
        vec![0, 1],
        DEFASAGEM,
        sem_tardios(),
        None,
        move |posicao, _| {
            let contador = Arc::clone(&contador);
            async move {
                contador.fetch_add(1, Ordering::SeqCst);
                // O primeiro nunca termina dentro do horizonte deste teste.
                let espera = if posicao == 0 { 100_000 } else { 10 };
                tokio::time::sleep(Duration::from_millis(espera)).await;
                Ok::<usize, ConnectError>(posicao)
            }
        },
    )
    .await;

    assert_eq!(corrida.vencedor.map(|(posicao, _)| posicao), Some(1));
    assert_eq!(
        comecaram.load(Ordering::SeqCst),
        2,
        "o segundo candidato não chegou a começar"
    );
}

/// Quando todos falham, todas as falhas voltam — quem chama precisa delas
/// para escolher a que melhor descreve o que houve, e para limpar os pins.
#[tokio::test(start_paused = true)]
async fn todas_as_falhas_voltam_quando_ninguem_fecha() {
    let corrida = correr(
        vec![0, 1, 2],
        DEFASAGEM,
        sem_tardios(),
        None,
        |_, _| async { Err::<usize, ConnectError>(ConnectError::Unreachable) },
    )
    .await;

    assert!(corrida.vencedor.is_none());
    assert_eq!(corrida.falhas.len(), 3);
}

/// Uma lista vazia não trava: devolve sem vencedor e sem falha.
#[tokio::test(start_paused = true)]
async fn uma_corrida_sem_candidatos_termina() {
    let corrida = correr(Vec::new(), DEFASAGEM, sem_tardios(), None, |_, _| async {
        Ok::<usize, ConnectError>(0)
    })
    .await;

    assert!(corrida.vencedor.is_none());
    assert!(corrida.falhas.is_empty());
}

/// Quem falha cedo não impede quem vem depois de vencer.
#[tokio::test(start_paused = true)]
async fn uma_falha_imediata_nao_encerra_a_corrida() {
    let corrida = correr(
        vec![0, 1],
        DEFASAGEM,
        sem_tardios(),
        None,
        |posicao, _| async move {
            if posicao == 0 {
                return Err(ConnectError::Unreachable);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<usize, ConnectError>(posicao)
        },
    )
    .await;

    assert_eq!(corrida.vencedor.map(|(posicao, _)| posicao), Some(1));
    assert_eq!(corrida.falhas.len(), 1);
}

/// Os candidatos começam um a cada defasagem, na ordem da lista: o
/// primeiro na hora, o segundo em 250 ms, o terceiro em 500 ms.
///
/// É a ordem de hoje escrita em números. Quem mexer na corrida tem de
/// manter estes três instantes quando ninguém chega depois.
#[tokio::test(start_paused = true)]
async fn os_candidatos_comecam_um_a_cada_defasagem_na_ordem_da_lista() {
    let (quando, tentar) = anotando(&[], &[]);
    let corrida = correr(vec![10, 11, 12], DEFASAGEM, sem_tardios(), None, tentar).await;

    assert!(corrida.vencedor.is_none());
    assert_eq!(
        *quando.lock().unwrap(),
        vec![(0, 10, 0), (1, 11, 250), (2, 12, 500)]
    );
}

/// **O candidato tardio que hoje se perde.** Todos os da lista falham na
/// hora, e o único que fecharia chega 100 ms depois — o endereço que o
/// quarto dá a quem volta a um servidor guardado. A corrida de antes acaba
/// quando a lista acaba, e ele chega a uma corrida que já terminou.
#[tokio::test(start_paused = true)]
async fn um_tardio_que_chega_depois_de_todos_falharem_ainda_corre() {
    let (_, tentar) = anotando(&[20], &[10, 11]);
    let corrida = correr(
        vec![10, 11],
        DEFASAGEM,
        tardios(Duration::from_millis(100), vec![20]),
        None,
        tentar,
    )
    .await;

    assert_eq!(
        corrida.vencedor.map(|(_, quem)| quem),
        Some(20),
        "o candidato tardio se perdeu: a corrida acabou antes de ele chegar"
    );
}

/// O tardio começa **na hora** em que chega, na frente de quem ainda espera
/// a vez, e a defasagem recomeça dele. As posições são a ordem em que cada
/// um começou, que é o que a trilha numera.
#[tokio::test(start_paused = true)]
async fn um_tardio_comeca_na_hora_e_na_frente_dos_que_esperam() {
    let (quando, tentar) = anotando(&[], &[]);
    let corrida = correr(
        vec![10, 11, 12],
        DEFASAGEM,
        tardios(Duration::from_millis(100), vec![20]),
        None,
        tentar,
    )
    .await;

    assert_eq!(
        *quando.lock().unwrap(),
        vec![(0, 10, 0), (1, 20, 100), (2, 11, 350), (3, 12, 600)]
    );
    assert_eq!(corrida.iniciados, vec![10, 20, 11, 12]);
}

/// Um quarto que não traz ninguém não mexe nos instantes: ninguém começa
/// fora da vez por causa dele.
#[tokio::test(start_paused = true)]
async fn um_tardio_vazio_nao_muda_a_ordem_de_hoje() {
    let (quando, tentar) = anotando(&[], &[]);
    let _ = correr(
        vec![10, 11, 12],
        DEFASAGEM,
        tardios(Duration::from_millis(100), Vec::new()),
        None,
        tentar,
    )
    .await;

    assert_eq!(
        *quando.lock().unwrap(),
        vec![(0, 10, 0), (1, 11, 250), (2, 12, 500)]
    );
}

/// Quem já ganhava continua ganhando, sem esperar quem ainda vai chegar.
#[tokio::test(start_paused = true)]
async fn quem_ja_ganhava_nao_espera_o_tardio() {
    let (_, tentar) = anotando(&[10], &[]);
    let comeco = tokio::time::Instant::now();
    let corrida = correr(
        vec![10, 11],
        DEFASAGEM,
        tardios(Duration::from_secs(60), vec![20]),
        None,
        tentar,
    )
    .await;

    assert_eq!(corrida.vencedor.map(|(_, quem)| quem), Some(10));
    assert!(
        comeco.elapsed() < Duration::from_millis(100),
        "quem ganhava esperou {:?} por um tardio",
        comeco.elapsed()
    );
}

/// Conta quando uma tentativa foi largada sem terminar.
struct Largada {
    quando: Arc<std::sync::Mutex<Vec<(usize, Duration)>>>,
    posicao: usize,
    comeco: tokio::time::Instant,
    terminou: bool,
}

impl Drop for Largada {
    fn drop(&mut self) {
        if !self.terminou {
            self.quando
                .lock()
                .unwrap()
                .push((self.posicao, self.comeco.elapsed()));
        }
    }
}

/// **Quem toca vence, e ninguém mais começa** (tarefa 16 da F7).
///
/// O 0 nunca responde; o 1 toca 10 ms depois de começar e só termina 20 s
/// depois, quando atendem; o 2 começaria aos 500 ms. O 2 não começa, o 0 é
/// largado no instante do toque (e não no fim da corrida), e o 1 vence.
#[tokio::test(start_paused = true)]
async fn quem_toca_vence_e_ninguem_mais_comeca() {
    let (avisa, tocados) = tokio::sync::mpsc::unbounded_channel();
    let comecaram = Arc::new(std::sync::Mutex::new(Vec::new()));
    let largadas = Arc::new(std::sync::Mutex::new(Vec::new()));
    let comeco = tokio::time::Instant::now();
    let tentar = {
        let comecaram = Arc::clone(&comecaram);
        let largadas = Arc::clone(&largadas);
        move |posicao: usize, _| {
            comecaram.lock().unwrap().push(posicao);
            let avisa = avisa.clone();
            let largada = Largada {
                quando: Arc::clone(&largadas),
                posicao,
                comeco,
                terminou: false,
            };
            async move {
                // O marcador inteiro, e não só o campo: é o `Drop` dele
                // que conta a tentativa largada.
                let mut largada = largada;
                if posicao == 1 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    let _ = avisa.send(1);
                    tokio::time::sleep(Duration::from_secs(20)).await;
                } else {
                    tokio::time::sleep(Duration::from_secs(100)).await;
                }
                largada.terminou = true;
                Ok::<usize, ConnectError>(posicao)
            }
        }
    };
    let corrida = correr(
        vec![0, 1, 2],
        DEFASAGEM,
        sem_tardios(),
        Some(tocados),
        tentar,
    )
    .await;

    assert_eq!(corrida.vencedor.map(|(posicao, _)| posicao), Some(1));
    assert_eq!(
        *comecaram.lock().unwrap(),
        vec![0, 1],
        "alguém começou depois do toque, e chegaria à sala como um segundo toque"
    );
    let largadas = largadas.lock().unwrap().clone();
    assert!(
        largadas.len() == 1
            && largadas
                .first()
                .is_some_and(|(posicao, quando)| *posicao == 0 && *quando < Duration::from_secs(1)),
        "quem corria não foi largado no instante do toque: {largadas:?}"
    );
}

/// **Depois do toque, a sessão de outro candidato não vence** (tarefa 16 da
/// F7). O 0 abre uma sessão no mesmo instante em que o 1 toca: quem tocou
/// já levou a corrida, e a sessão do 0 é largada.
#[tokio::test(start_paused = true)]
async fn depois_do_toque_a_sessao_de_outro_nao_vence() {
    let (avisa, tocados) = tokio::sync::mpsc::unbounded_channel();
    let corrida = correr(
        vec![0, 1],
        DEFASAGEM,
        sem_tardios(),
        Some(tocados),
        move |posicao: usize, _| {
            let avisa = avisa.clone();
            async move {
                if posicao == 0 {
                    // Pronta no mesmo instante do toque do 1, que começa
                    // na vez dele e toca 10 ms depois.
                    tokio::time::sleep(DEFASAGEM + Duration::from_millis(10)).await;
                    return Ok::<usize, ConnectError>(0);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
                let _ = avisa.send(1);
                tokio::time::sleep(Duration::from_secs(20)).await;
                Ok(1)
            }
        },
    )
    .await;

    assert_eq!(
        corrida.vencedor.map(|(posicao, _)| posicao),
        Some(1),
        "a sessão de quem não tocou venceu a de quem estava tocando"
    );
}

/// **Quem toca e não é atendido leva a corrida junto** (tarefa 16 da F7).
///
/// O 0 toca e, 5 s depois, a sala diz que ninguém atendeu; o 1 entraria aos
/// 300 ms. Ligar de novo pelo outro caminho seria tocar outra vez para quem
/// não atendeu: a corrida acaba com o desfecho de quem tocou.
#[tokio::test(start_paused = true)]
async fn quem_toca_e_nao_e_atendido_leva_a_corrida_junto() {
    let (avisa, tocados) = tokio::sync::mpsc::unbounded_channel();
    let ninguem = ConnectError::Refused {
        reason: seele_proto::control::DisconnectReason::NinguemAtendeu,
    };
    let recusa = ninguem.clone();
    let corrida = correr(
        vec![0, 1],
        DEFASAGEM,
        sem_tardios(),
        Some(tocados),
        move |posicao: usize, _| {
            let avisa = avisa.clone();
            let recusa = recusa.clone();
            async move {
                if posicao == 0 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    let _ = avisa.send(0);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    return Err(recusa);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok::<usize, ConnectError>(posicao)
            }
        },
    )
    .await;

    assert!(
        corrida.vencedor.is_none(),
        "outro candidato entrou depois de o toque não ser atendido"
    );
    assert_eq!(corrida.falhas, vec![(0, ninguem)]);
}

/// **O prazo do candidato se solta quando ele toca** (tarefa 16 da F7): sem
/// toque, a tentativa que passa do prazo é largada nele; com toque, ela vai
/// até o fim, e quem ouve sabe uma vez, com o prazo do toque.
#[tokio::test(start_paused = true)]
async fn o_prazo_do_candidato_se_solta_quando_ele_toca() {
    let prazo = Duration::from_secs(4);

    let (_sem_toque, mut ninguem_toca) = tokio::sync::mpsc::unbounded_channel::<u16>();
    let sem_toque = super::ate_o_prazo_ou_o_toque(
        Some(prazo),
        tokio::time::sleep(Duration::from_secs(10)),
        &mut ninguem_toca,
        |_| panic!("ninguém tocou"),
    )
    .await;
    assert!(sem_toque.is_none(), "a tentativa sem toque passou do prazo");

    let (toca, mut ouviu) = tokio::sync::mpsc::unbounded_channel::<u16>();
    let ouvidos = std::sync::Mutex::new(Vec::new());
    let com_toque = super::ate_o_prazo_ou_o_toque(
        Some(prazo),
        async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = toca.send(45);
            tokio::time::sleep(Duration::from_secs(9)).await;
            "atendida"
        },
        &mut ouviu,
        |prazo_s| ouvidos.lock().unwrap().push(prazo_s),
    )
    .await;
    assert_eq!(
        com_toque,
        Some("atendida"),
        "quem tocava caiu no prazo do candidato"
    );
    assert_eq!(*ouvidos.lock().unwrap(), vec![45]);
}
