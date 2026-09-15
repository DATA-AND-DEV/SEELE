//! End-to-end protocol conformance tests.
//!
//! This crate is deliberately empty of product code. Everything that is tested
//! lives in `tests/`, because the point is not to ship code but to hold the one
//! place where `seele-server` and `seele-core` are allowed to meet.
//!
//! ADR 0002 forbids either from depending on the other: the daemon must not link
//! the headless client, and the client must not link the daemon. An end-to-end
//! test needs both, so it gets its own crate rather than a hole in the rule.
//!
//! A única coisa que mora aqui é o portão de vagas — ver [`vaga`] —, porque ele
//! precisa ser o mesmo objeto para todos os binários de teste deste crate.

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

/// Quanto tempo se espera por uma vaga antes de seguir assim mesmo.
///
/// O portão é uma conveniência, não uma regra: se por qualquer motivo as vagas
/// não forem liberadas — um processo de teste pendurado de uma rodada anterior,
/// um sistema de arquivos sem trava —, é melhor a suíte correr saturada do que
/// travar para sempre esperando uma vaga que não vem.
///
/// **Vinte segundos, e não noventa.** Foi medido: com outra árvore de trabalho
/// rodando `cargo test -p seele-conformance -- --test-threads=1` ao lado, a
/// thread única daquela rodada segura uma vaga do começo ao fim, por dezenas de
/// minutos. Como o `cargo` roda os alvos de teste um de cada vez, uma espera
/// longa demais é paga **uma vez por alvo**: noventa segundos viram meia hora de
/// relógio ao longo dos 25 alvos deste crate, e foi isso que estourou o prazo da
/// validação automática. Vinte segundos ainda cobrem com folga a espera que o
/// portão existe para cobrir — a de um teste vizinho desta mesma suíte
/// terminando —, e o teto do estrago cai junto. Ver também [`DESCANSO`].
const ESPERA_MAXIMA: Duration = Duration::from_secs(20);

/// Quanto tempo uma desistência cala a espera dos alvos seguintes.
///
/// Esperar o prazo inteiro e não conseguir vaga é notícia: quem está segurando o
/// conjunto não é esta rodada, e não vai soltar por educação. Sem isto, o alvo
/// seguinte redescobre a mesma saturação do zero, e o seguinte também — o custo
/// é o prazo vezes o número de alvos. Com isto, a rodada paga a descoberta uma
/// vez e depois segue saturada, que é exatamente o comportamento de antes do
/// portão: a degradação fica limitada em vez de somar.
///
/// A marca só desliga a **espera**, nunca a **tomada**: um alvo que chegue com o
/// conjunto livre continua pegando a sua vaga na hora. E ela mora na pasta das
/// vagas, então caduca sozinha e vale para quem estiver disputando o mesmo
/// conjunto, que é quem tem o mesmo problema.
const DESCANSO: Duration = Duration::from_secs(60);

/// De quanto em quanto tempo se varre o conjunto de vagas outra vez.
const INTERVALO: Duration = Duration::from_millis(20);

thread_local! {
    /// A vaga desta thread, se ela já tiver uma.
    ///
    /// É `thread_local` de propósito, e por dois motivos. O primeiro é a
    /// liberação: o `libtest` roda cada teste na sua própria thread, então a
    /// trava cai sozinha quando o teste termina, sem que ninguém precise
    /// devolvê-la à mão nem mudar a assinatura de nenhum ajudante. O segundo é
    /// `--test-threads=1`: nesse modo o `libtest` roda tudo na thread principal,
    /// e uma vaga por teste esgotaria o conjunto no enésimo teste e travaria a
    /// suíte. Como a vaga é da thread e não da chamada, ali ela é tomada uma vez
    /// e reusada por todos — que é exatamente o que se quer de uma execução que
    /// já é serial.
    static MINHA_VAGA: std::cell::RefCell<Option<File>> = const { std::cell::RefCell::new(None) };
}

/// Quantos servidores de conformidade podem existir ao mesmo tempo nesta máquina.
///
/// Um terço dos núcleos, nunca menos que dois. O número sai da medida da
/// pendência 29: cada teste daqui levanta um servidor QUIC de verdade — aperto
/// de mão, TLS e banco —, e em paralelo total o crate leva *mais* tempo do que
/// em série, porque o que se ganha em sobreposição se perde em contenção. Dois
/// é o piso para que a suíte não vire serial numa máquina pequena.
///
/// `SEELE_VAGAS` manda nisto quando está no ambiente, e `SEELE_VAGAS=0` desliga
/// o portão inteiro. Não é conforto de configuração: é o que torna a prova de
/// reversão deste conserto **reproduzível** por quem vier depois — rodar a
/// suíte com o portão desligado é rodar a suíte de antes dele, sem precisar
/// reverter nada nem confiar na palavra de quem mediu.
fn vagas() -> usize {
    if let Some(pedido) = std::env::var("SEELE_VAGAS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
    {
        return pedido;
    }
    std::thread::available_parallelism()
        .map(|n| (n.get() / 3).max(2))
        .unwrap_or(2)
}

/// Toma uma vaga para esta thread, esperando se todas estiverem ocupadas.
///
/// **Por que isto existe.** Pendência 29: `cargo test` do workspace reprovava um
/// teste de conformidade por rodada, sempre um diferente, sempre estourando um
/// prazo de ~20 s — e sempre passando sozinho em menos de um segundo. Não era
/// regressão nenhuma: o `cargo` roda os binários de teste em paralelo, cada
/// binário roda seus testes em paralelo de novo, e o prazo por candidato do
/// `connect` (`PRAZO_POR_CANDIDATO`, em `seele-core/src/enlace.rs`) é relógio de
/// parede. Numa máquina saturada ele queima sem que nada esteja quebrado, e uma
/// suíte que reprova ao acaso deixa de ser evidência.
///
/// **Por que trava de arquivo e não semáforo.** Um semáforo em memória só
/// enxergaria as threads do próprio binário, e a saturação vem sobretudo de
/// binários diferentes correndo ao mesmo tempo, que são processos diferentes. A
/// trava de arquivo é o único cadeado que os três sistemas operacionais
/// compartilham entre processos — e, melhor ainda, o sistema a devolve sozinho
/// quando o processo morre, inclusive se ele morrer de pânico.
///
/// Chamar duas vezes na mesma thread não toma duas vagas: a segunda chamada vê
/// que a thread já tem a sua e volta na hora.
///
/// **O conjunto é da máquina inteira, e isso é de propósito.** As vagas moram na
/// pasta temporária do sistema, então duas árvores de trabalho rodando a bateria
/// ao mesmo tempo disputam o mesmo conjunto e se serializam uma contra a outra.
/// Não é descuido de escopo: o que satura é a máquina — núcleos e portas
/// efêmeras —, e um conjunto por árvore daria a cada uma o direito de levantar o
/// seu tanto de servidores QUIC, que somados são exatamente a saturação que o
/// portão existe para evitar. Quem quiser as árvores independentes tem
/// `SEELE_VAGAS` para repartir o teto entre elas, ou `SEELE_VAGAS=0` para
/// desligar o portão de uma delas.
///
/// **Chame daqui de dentro do corpo do teste, nunca de dentro de um
/// `tokio::spawn`.** Nos testes `#[tokio::test(flavor = "multi_thread")]` o
/// `block_on` conduz o futuro na própria thread do `libtest`, então uma chamada
/// aguardada do corpo do teste — ou de um ajudante que o corpo aguarda, que é o
/// caso de todos os pontos de chamada de hoje — acontece nessa thread: uma vaga
/// por teste, devolvida quando a thread acaba. Uma chamada de dentro de uma
/// tarefa disparada correria numa thread de trabalho, tomaria uma segunda vaga
/// para o mesmo teste e esperaria bloqueando dentro do executor. Como toda
/// chamada fica no preparo do servidor, antes do primeiro `spawn`, não há
/// tarefa alguma para essa espera atrasar.
pub fn vaga() {
    MINHA_VAGA.with(|minha| {
        let mut minha = minha.borrow_mut();
        if minha.is_some() {
            return;
        }
        *minha = tomar();
    });
}

/// Varre o conjunto de vagas até conseguir uma, ou até o tempo acabar.
fn tomar() -> Option<File> {
    let total = vagas();
    // Portão desligado — ver `SEELE_VAGAS`. Segue sem vaga em vez de esperar
    // por uma que não existe.
    if total == 0 {
        return None;
    }
    let pasta = std::env::temp_dir().join("seele-conformance-vagas");
    if std::fs::create_dir_all(&pasta).is_err() {
        return None;
    }
    tomar_em(&pasta, total, ESPERA_MAXIMA)
}

/// O miolo de [`tomar`], com a pasta e o prazo de fora.
///
/// Separado para que o teto do estrago do portão possa ser **provado** e não
/// apenas afirmado: o teste toma a única vaga de uma pasta sua e cronometra as
/// chamadas seguintes. Com o prazo vindo de fora, a prova cabe em décimos de
/// segundo em vez dos vinte segundos de produção.
fn tomar_em(pasta: &Path, total: usize, espera: Duration) -> Option<File> {
    // Alguém já esperou o prazo inteiro por este conjunto e não conseguiu. Vale
    // tentar uma volta — vaga livre se pega na hora —, mas não vale esperar de
    // novo pelo que já se sabe que não vem. Ver `DESCANSO`.
    let esperar = !desistencia_recente(pasta, SystemTime::now());
    let limite = Instant::now() + espera;
    loop {
        for indice in 0..total {
            let caminho = pasta.join(format!("vaga-{indice}.lock"));
            let Ok(arquivo) = OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&caminho)
            else {
                // Sem poder abrir o arquivo não há portão; segue sem vaga.
                return None;
            };
            match arquivo.try_lock() {
                Ok(()) => return Some(arquivo),
                Err(std::fs::TryLockError::WouldBlock) => continue,
                // Sistema de arquivos sem trava (alguns NFS). Segue sem vaga,
                // que é o comportamento de antes deste portão.
                Err(std::fs::TryLockError::Error(_)) => return None,
            }
        }
        if !esperar || Instant::now() >= limite {
            if esperar {
                // Esperamos o prazo inteiro e o conjunto continua alheio. Deixa
                // dito, para os alvos seguintes desta rodada não repetirem a
                // descoberta um por um.
                marcar_desistencia(pasta);
            }
            return None;
        }
        std::thread::sleep(INTERVALO);
    }
}

/// O caminho da marca de desistência dentro da pasta das vagas.
fn marca(pasta: &Path) -> std::path::PathBuf {
    pasta.join("desistencia")
}

/// Alguém desistiu deste conjunto de vagas há pouco?
///
/// «Há pouco» é [`DESCANSO`]. Recebe o agora de fora para o teste poder
/// envelhecer a marca sem dormir.
fn desistencia_recente(pasta: &Path, agora: SystemTime) -> bool {
    let Ok(quando) = std::fs::metadata(marca(pasta)).and_then(|m| m.modified()) else {
        return false;
    };
    // Um relógio que andou para trás deixa a marca no futuro. Tratar como
    // recente é o lado seguro: no pior caso se espera menos.
    agora
        .duration_since(quando)
        .map(|idade| idade < DESCANSO)
        .unwrap_or(true)
}

/// Registra que o prazo de espera queimou sem vaga.
///
/// Falhar aqui não é motivo de pânico: sem a marca o portão volta a custar o
/// prazo por alvo, que é o que ele custava antes, e não quebra nada.
fn marcar_desistencia(pasta: &Path) {
    // `create` trunca e, com isso, carimba a data de agora; é a data que
    // interessa, e o conteúdo não interessa nenhum.
    let _ = File::create(marca(pasta));
}

#[cfg(test)]
mod tests {
    // Mesma liberação que cada arquivo de `tests/` deste crate já faz: num
    // teste, `expect` é a forma de reprovar com a frase certa.
    #![allow(clippy::expect_used)]

    use super::*;

    /// O teto do estrago, cronometrado.
    ///
    /// Com o conjunto tomado por outra rodada — aqui, por uma vaga que este
    /// mesmo teste segura —, a primeira chamada paga o prazo e as seguintes não
    /// pagam mais nada. É o que separa «a suíte fica um pouco mais lenta numa
    /// máquina cheia» de «a suíte estoura o prazo de quem a chamou»: são 25
    /// alvos, e sem isto cada um deles paga o prazo inteiro por conta própria.
    #[test]
    fn o_alvo_seguinte_nao_paga_a_espera_de_novo_quando_a_vaga_e_de_outro() {
        let pasta = pasta_de_teste("teto");
        let espera = Duration::from_millis(300);

        // A única vaga do conjunto fica presa nesta chamada — é o papel da
        // árvore de trabalho vizinha que segura o conjunto por meia hora.
        let presa = tomar_em(&pasta, 1, espera).expect("a primeira vaga estava livre");

        let relogio = std::time::Instant::now();
        assert!(
            tomar_em(&pasta, 1, espera).is_none(),
            "saiu vaga de um conjunto de uma vaga só que já estava tomada"
        );
        let primeira = relogio.elapsed();
        assert!(
            primeira >= espera,
            "a primeira desistência não chegou a esperar ({primeira:?} de \
             {espera:?}); o portão deixaria de cobrir a espera curta que ele \
             existe para cobrir"
        );

        let relogio = std::time::Instant::now();
        assert!(
            tomar_em(&pasta, 1, espera).is_none(),
            "saiu vaga de um conjunto de uma vaga só que já estava tomada"
        );
        let segunda = relogio.elapsed();
        assert!(
            segunda < espera / 3,
            "o alvo seguinte voltou a pagar a espera inteira ({segunda:?} de \
             {espera:?}).\n\
             Com 25 alvos neste crate, essa conta é a diferença entre uma \
             bateria que cabe no prazo da validação e uma que estoura em \
             espera de vaga alheia."
        );

        drop(presa);
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// Uma pasta de vagas só deste teste, para não disputar a da máquina.
    fn pasta_de_teste(nome: &str) -> std::path::PathBuf {
        let pasta =
            std::env::temp_dir().join(format!("seele-vagas-teste-{}-{nome}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).expect("pasta de teste");
        pasta
    }

    /// A desistência cala a espera dos alvos seguintes, e caduca sozinha.
    ///
    /// É o teto do estrago do portão: sem isto, cada um dos 25 alvos deste crate
    /// paga o prazo de espera inteiro quando outra árvore de trabalho está
    /// segurando as vagas, e a soma estoura o prazo de quem chamou a bateria.
    #[test]
    fn quem_ja_desistiu_nao_faz_o_alvo_seguinte_esperar_de_novo() {
        let pasta = pasta_de_teste("caducidade");

        let agora = SystemTime::now();
        // Sem marca nenhuma não há desistência: espera-se normalmente.
        assert!(
            !desistencia_recente(&pasta, agora),
            "pasta limpa não pode parecer uma desistência; o portão deixaria de \
             esperar logo na primeira vez, que é justamente quando esperar vale"
        );

        marcar_desistencia(&pasta);
        assert!(
            desistencia_recente(&pasta, agora),
            "a desistência recém-registrada não foi vista pelo alvo seguinte; \
             cada alvo da suíte voltaria a pagar o prazo de espera inteiro"
        );

        // Envelhecida além do descanso, a marca deixa de valer — senão uma
        // saturação de um minuto atrás desligaria o portão para sempre.
        let velha = agora - DESCANSO - Duration::from_secs(1);
        let _ = std::fs::File::open(marca(&pasta))
            .and_then(|f| f.set_times(std::fs::FileTimes::new().set_modified(velha)));
        assert!(
            !desistencia_recente(&pasta, agora),
            "uma desistência velha continuou calando a espera; o portão ficaria \
             desligado muito depois de a máquina ter desafogado"
        );

        let _ = std::fs::remove_dir_all(&pasta);
    }
}
