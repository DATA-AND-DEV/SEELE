//! Roda as bancadas de MOD que precisam só de Node.
//!
//! Todas carregam o arquivo de verdade em vez de uma cópia, num contexto de
//! VM, com o resto de mentira. Entre elas:
//!
//! - `ciclo-do-executor.cjs` põe `ui/mods-runtime.js` num contexto de VM e
//!   controla a **ordem dos eventos** — que é a única coisa que separa um
//!   caminho correto de um que só parece correto;
//! - `regiao-do-mod.cjs` põe `ui/mods-regiao.js` num DOM mínimo e mede o que um
//!   guarda de texto não alcança: que atualizar não tira do documento quem tem
//!   foco, que sair **para o som** em vez de só tirar o nó da tela, e que um
//!   cartão na lista de pessoas usa o mesmo renderer com gramática e tetos
//!   próprios — e sai da lista quando o MOD sai. E que cada mídia recusada é
//!   dita a quem hospeda, com o motivo;
//! - `continuacao-de-midia.cjs` extrai de `ui/base.js` o laço que busca uma
//!   mídia grande do servidor de um MOD e o roda contra um servidor de mentira:
//!   a ordem dos pedaços, o teto de voltas e a falha nomeada;
//! - `contribuicoes-e-camadas.cjs` monta a árvore do **`index.html` de
//!   verdade** e roda o roteador real contra ela. É a inversão das reproduções
//!   da revisão de 20/09/2026: um modal que não adormece o próprio ancestral,
//!   uma superfície que volta ao documento ao ser mostrada, três estados de
//!   apresentação em vez de dois, mil ciclos que não retêm nada, e um MOD que
//!   não revoga o que é de outro nem alcança mensagem de versão que ele não
//!   declara. E que o que não coube numa contribuição ou num cartão chega ao
//!   registro como aviso e com o id do MOD, e o avatar que não montou, à
//!   `anotarRecusa` que o leva até lá;
//! - `envio-de-imagens.cjs` extrai de `ui/base.js` a fila dos pedidos de MOD ao
//!   servidor e a roda com relógio virtual contra o balde de controle do
//!   servidor: dois envios de 10 MiB ao mesmo tempo, com o resto do tráfego,
//!   cabem no balde, e um pedido da sessão anterior não atravessa;
//! - `imagens-por-volume.cjs` roda a ponte de `ui/base.js` e o prelúdio de
//!   `crates/seele-motor/src/preludio.rs`: o envio de imagem é recusado nas
//!   APIs 3 e 4, vai com o id e a geração de quem hospeda e não com os que o
//!   MOD manda, só confirma
//!   depois de gravar, responde `sessao-encerrada` quando o MOD deixa de valer
//!   no meio da gravação, e só existe no SDK com a capacidade `volume`;
//! - `ponte-web.cjs` põe `ui/ponte-web.js` numa janela de mentira e confere o
//!   que a ponte do navegador responde: o lugar (o Tauri, um duble e o
//!   navegador), a marca da casca, as recusas de cada casca, os neutros de
//!   quem não hospeda, o `disconnect` que sai e a abertura que nunca entra.
//!
//! As de navegador (Playwright) não estão aqui: elas precisam de um Chromium,
//! e quem as roda, por nome, é o job `bancadas` do `ci.yml`, que só roda
//! quando alguém o dispara — nem o `validar` do `release.yml` nem a bateria do
//! `publicar.sh` as rodam. A exceção é a do executor de MODs no navegador
//! (`executor-no-navegador.cjs`), que precisa do `.wasm` do motor: ela roda no
//! job `bancadas-de-mod`, depois do `cargo xtask web` que o compila. Quantas
//! são e onde cada uma roda não está escrito em comentário nenhum: o teste
//! `toda_bancada_da_pasta_roda_em_algum_lugar`, abaixo, exige que todo `.cjs`
//! da pasta esteja nesta lista ou num `run:` de um daqueles dois jobs. Ficam
//! de fora só o `playwright.cjs`, que carrega o Playwright,
//! e o `telas.cjs`, que é o servidor que as de navegador usam e, rodado
//! sozinho, uma ferramenta de retrato: tira a foto de uma cena e não julga o
//! que vê.
//!
//! # Quem a chama
//!
//! Esta lista é a única. Três lugares a chamam, e nenhum repete os nomes:
//!
//! - a bateria do `empacotar/publicar.sh`, que é por onde as versões vão para o
//!   SEELE-RELEASES; o `--sem-bateria` a pula junto com o resto;
//! - o `validar` do `release.yml`, no Linux, depois dos testes;
//! - o job `bancadas-de-mod` do `ci.yml`.
//!
//! Uma bancada nova de Node puro entra aqui, e os três passam a rodá-la. Os
//! testes deste módulo reprovam se algum deles deixar de chamar a lista ou
//! voltar a nomear uma delas, se o passo dos workflows perder o prazo ou, no
//! `validar`, voltar para antes dos testes, e se uma bancada da pasta não
//! estiver nem aqui nem no job das de navegador.
//!
//! Ela diz o nome de cada bancada **antes** de rodá-la
//! (`check-runtime: rodando <nome>`): uma que pendura de verdade — um
//! temporizador vivo, um laço — não reprova, quem a para é o prazo de quem
//! chama a lista, e a última linha dita é a que a nomeia.
//!
//! # Por que aqui, e não em `cargo test`
//!
//! Porque ela precisa de Node, e a bateria do produto não pode precisar: quem
//! roda `cargo test` é quem compila o SEELE, e o SEELE não usa Node para nada.
//! Uma bateria que reprova por falta de uma ferramenta que o produto não usa é
//! uma bateria que ensina a ignorá-la.
//!
//! Em `xtask` ela é obrigatória e explícita: `cargo xtask check-runtime`
//! reprova sem Node, porque quem chama esta ferramenta pediu por ela.
//!
//! # E os roteiros da casca
//!
//! Os `tools/roteiros/*.js` não são bancadas e não rodam por esta lista: eles
//! rodam pela `tools/carga-da-casca.py`, num `chrome-headless-shell`, no job
//! `bancadas` do `ci.yml` e na bateria do `publicar.sh` (lote RT da F5). O
//! teste `todo_roteiro_da_pasta_roda_no_ci_e_no_publicar`, abaixo, mora aqui
//! porque é o mesmo molde do `toda_bancada_da_pasta_roda_em_algum_lugar`: ele
//! exige cada arquivo da pasta nas duas chamadas da carga.

use std::path::PathBuf;
use std::process::ExitCode;

/// As bancadas, por nome e não por um laço sobre a pasta: um rascunho guardado
/// em `bancada/` não pode passar a reprovar os três lugares que a chamam, e as
/// de navegador moram na mesma pasta.
const BANCADAS: [&str; 13] = [
    "ciclo-do-executor.cjs",
    "ponte-web.cjs",
    "regiao-do-mod.cjs",
    "continuacao-de-midia.cjs",
    "contribuicoes-e-camadas.cjs",
    "envio-de-imagens.cjs",
    "imagens-por-volume.cjs",
    "tema-sai-com-a-sessao.cjs",
    "retrato-projetado.cjs",
    "eventos-da-tela.cjs",
    "tons-do-tema.cjs",
    "chaves-do-tema.cjs",
    "hospedar-com-mod-antigo.cjs",
];

fn raiz() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default()
}

pub(crate) fn run() -> ExitCode {
    let passou = rodar(&raiz(), &BANCADAS, &mut std::io::stderr(), |bancada| {
        std::process::Command::new("node")
            .arg(bancada)
            .status()
            .map(|estado| (!estado.success()).then(|| estado.to_string()))
    });
    if passou {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Roda `bancadas` com `node`, dizendo em `saida` o que acontece, e responde se
/// todas passaram.
///
/// `node` roda uma bancada e devolve `None` quando ela passa, ou o estado com
/// que ela reprovou. É um parâmetro para que os testes meçam a ordem do que a
/// lista diz sem precisar de Node.
fn rodar(
    raiz: &std::path::Path,
    bancadas: &[&str],
    saida: &mut dyn std::io::Write,
    mut node: impl FnMut(&std::path::Path) -> std::io::Result<Option<String>>,
) -> bool {
    // **Todas, e não até a primeira que reprova.** Saber que duas quebraram é
    // uma informação diferente de saber que uma quebrou, e é a que diz se a
    // mudança foi num lugar ou na fronteira entre os dois.
    //
    // O que não se consegue escrever em `saida` não para a lista: o veredito
    // é o código de saída, e não estas linhas.
    let mut falhou = false;
    for nome in bancadas {
        let bancada = raiz.join("apps/seele-app/bancada").join(nome);
        if !bancada.is_file() {
            let _ = writeln!(
                saida,
                "check-runtime: a bancada sumiu de {}",
                bancada.display()
            );
            return false;
        }
        // **Dita antes de rodar.** Uma que pendura de verdade não reprova: o
        // prazo de quem chama a lista a mata, e esta é a última linha que a
        // lista disse.
        let _ = writeln!(saida, "check-runtime: rodando {nome}");
        match node(&bancada) {
            Ok(None) => {}
            Ok(Some(estado)) => {
                let _ = writeln!(saida, "check-runtime: {nome} reprovou ({estado})");
                falhou = true;
            }
            // **Nomeado, e não engolido.** Sem Node não dá para rodar, e dizer
            // «passou» aqui seria a pior resposta possível: quem chamou acharia
            // que as corridas foram provadas.
            Err(erro) => {
                let _ = writeln!(saida, "check-runtime: não consegui rodar o `node`: {erro}");
                let _ = writeln!(
                    saida,
                    "  a bancada precisa dele, e o produto não — ver o topo de xtask/src/check_runtime.rs"
                );
                return false;
            }
        }
    }
    !falhou
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]
mod tests {
    use super::*;

    /// Os três lugares que chamam a lista.
    const LUGARES: [&str; 3] = [
        "empacotar/publicar.sh",
        ".github/workflows/release.yml",
        ".github/workflows/ci.yml",
    ];

    /// As linhas do arquivo sem o que vem depois de `#`.
    ///
    /// Os comentários desses arquivos explicam por que a lista mora aqui, e um
    /// guarda que lesse o arquivo inteiro casaria com a própria explicação.
    fn sem_comentario(texto: &str) -> Vec<&str> {
        texto
            .lines()
            .map(|linha| linha.split('#').next().unwrap_or_default())
            .collect()
    }

    /// A linha que **chama** a lista, e não uma frase que a cita: no
    /// `publicar.sh` ela é o argumento de `etapa_da_bateria`, e nos workflows o
    /// `run:` do passo.
    fn chama_a_lista(linha: &str) -> bool {
        let linha = linha.trim();
        let linha = linha.strip_prefix("- ").unwrap_or(linha).trim_start();
        let linha = linha.strip_prefix("run:").unwrap_or(linha).trim();
        linha == "cargo xtask check-runtime"
    }

    fn ler(caminho: &str) -> String {
        std::fs::read_to_string(raiz().join(caminho))
            .expect("quem chama a lista tem que ser legível")
    }

    #[test]
    fn os_tres_lugares_chamam_esta_lista_e_nenhum_repete_os_nomes() {
        for caminho in LUGARES {
            let texto = ler(caminho);
            let linhas = sem_comentario(&texto);
            assert!(
                linhas.iter().any(|linha| chama_a_lista(linha)),
                "{caminho} deixou de chamar `cargo xtask check-runtime`, e as bancadas de MOD \
                 param de rodar ali sem que nada fique vermelho"
            );
            for nome in BANCADAS {
                assert!(
                    !linhas.iter().any(|linha| linha.contains(nome)),
                    "{caminho} voltou a nomear {nome}: a lista passa a ter duas casas, e a \
                     próxima bancada entra numa e não na outra"
                );
            }
        }
    }

    fn recuo(linha: &str) -> usize {
        linha.len() - linha.trim_start().len()
    }

    /// O passo do workflow que chama a lista, inteiro: da linha `- ` que o abre
    /// até a primeira linha com recuo igual ou menor, que é o passo seguinte ou o
    /// fim do job. Devolve também a linha da chamada, para medir a ordem.
    fn passo_da_lista(caminho: &str) -> (usize, String) {
        let texto = ler(caminho);
        let linhas = sem_comentario(&texto);
        let chamada = linhas
            .iter()
            .position(|linha| chama_a_lista(linha))
            .expect("o workflow chama a lista — o teste acima diz se não");
        let inicio = linhas
            .iter()
            .take(chamada + 1)
            .rposition(|linha| linha.trim_start().starts_with("- "))
            .expect("todo passo começa por `- `");
        let recuo_do_passo = linhas.get(inicio).map_or(0, |linha| recuo(linha));
        let fim = linhas
            .iter()
            .enumerate()
            .skip(inicio + 1)
            .find(|(_, linha)| !linha.trim().is_empty() && recuo(linha) <= recuo_do_passo)
            .map_or(linhas.len(), |(onde, _)| onde);
        let passo = linhas.iter().skip(inicio).take(fim - inicio);
        (chamada, passo.copied().collect::<Vec<_>>().join("\n"))
    }

    /// Os arquivos de `bancada/` que não são bancada: o carregador do
    /// Playwright e o `telas.cjs`, que é o servidor que as de navegador usam e,
    /// rodado sozinho, uma ferramenta de retrato que não julga o que vê.
    const NAO_SAO_BANCADA: [&str; 2] = ["playwright.cjs", "telas.cjs"];

    /// As bancadas que não rodam em lugar nenhum, **de propósito e por nome**.
    ///
    /// Vazia. Uma bancada que entra aqui é uma prova que ninguém roda, e o
    /// nome escrito neste lugar é o que faz isso ser uma decisão e não um
    /// esquecimento.
    const SEM_LUGAR: [&str; 0] = [];

    /// Os jobs do `ci.yml` que rodam bancadas de navegador: o `bancadas`, de
    /// todas elas, e o `bancadas-de-mod`, da que precisa do `.wasm` do motor
    /// que o `cargo xtask web` dele compila.
    const JOBS_COM_BANCADA_DE_NAVEGADOR: [&str; 2] = ["bancadas", "bancadas-de-mod"];

    /// Os passos de um job do `ci.yml`, cada um inteiro e sem comentário: o
    /// job vai da linha `  <job>:` até o próximo job, que é a próxima linha com
    /// o recuo de um job, e cada passo vai da linha `- ` que o abre até a
    /// próxima `- ` com o mesmo recuo.
    fn passos_do_job(job: &str) -> Vec<String> {
        let texto = ler(".github/workflows/ci.yml");
        passos_de(job, &sem_comentario(&texto))
    }

    /// Os mesmos passos, **com** o que vem depois de `#`: é o texto que o YAML
    /// entrega. Dentro de um bloco escalar (`>-`, `|`) o `#` não é comentário
    /// do YAML, é conteúdo — e chega ao shell, que o lê como comentário dele e
    /// corta o resto da linha. Ler o `run:` sem ele é ler um comando que o
    /// Actions não roda (I1 da revisão do lote RT).
    fn passos_do_job_como_o_yaml_os_entrega(job: &str) -> Vec<String> {
        let texto = ler(".github/workflows/ci.yml");
        passos_de(job, &texto.lines().collect::<Vec<_>>())
    }

    /// Os passos do job `job` nas `linhas` do `ci.yml`.
    fn passos_de(job: &str, linhas: &[&str]) -> Vec<String> {
        let inicio = linhas
            .iter()
            .position(|linha| linha.trim_end() == format!("  {job}:"))
            .unwrap_or_else(|| panic!("o `ci.yml` tem o job `{job}`"));
        let mut passos: Vec<Vec<&str>> = Vec::new();
        let mut recuo_dos_passos = None;
        for linha in linhas
            .iter()
            .skip(inicio + 1)
            .take_while(|linha| linha.trim().is_empty() || recuo(linha) > 2)
        {
            let abre = linha.trim_start().starts_with("- ")
                && recuo_dos_passos.is_none_or(|recuo_deles| recuo(linha) <= recuo_deles);
            if abre {
                recuo_dos_passos.get_or_insert(recuo(linha));
                passos.push(vec![linha]);
            } else if let Some(passo) = passos.last_mut() {
                passo.push(linha);
            }
        }
        passos.into_iter().map(|passo| passo.join("\n")).collect()
    }

    /// O valor da chave `chave:` do passo, de qualquer linha dele (a primeira
    /// vem depois do `- `).
    fn chave_do_passo<'a>(passo: &'a str, chave: &str) -> Option<&'a str> {
        passo.lines().find_map(|linha| {
            let linha = linha.trim();
            let linha = linha.strip_prefix("- ").unwrap_or(linha).trim_start();
            linha.strip_prefix(chave)?.strip_prefix(':').map(str::trim)
        })
    }

    /// A bancada que o passo roda, pelo `run: node apps/seele-app/bancada/<nome>`,
    /// com ou sem argumentos depois do nome.
    fn bancada_do_passo(passo: &str) -> Option<String> {
        chave_do_passo(passo, "run")?
            .strip_prefix("node apps/seele-app/bancada/")?
            .split_whitespace()
            .next()
            .map(str::to_owned)
    }

    /// As bancadas que os jobs de navegador do `ci.yml` rodam, pelo `run:` de
    /// cada uma.
    fn bancadas_dos_jobs_de_navegador() -> Vec<String> {
        JOBS_COM_BANCADA_DE_NAVEGADOR
            .iter()
            .flat_map(|job| passos_do_job(job))
            .filter_map(|passo| bancada_do_passo(&passo))
            .collect()
    }

    /// **Toda bancada da pasta roda em algum lugar.**
    ///
    /// Três bancadas de navegador — `ajustes-v013.cjs`,
    /// `escolha-de-versao-de-mod.cjs` e `gif-na-conversa.cjs` — ficaram de
    /// 21/09/2026 a 01/10/2026 sem rodar em lugar nenhum, e quem dizia quais
    /// rodavam era um comentário, que nada mantinha verdadeiro. Este teste é a
    /// lista: cada `.cjs` de `bancada/` está em [`BANCADAS`] (Node puro), num
    /// `run: node apps/seele-app/bancada/<nome>` do job `bancadas` ou do
    /// `bancadas-de-mod` do `ci.yml` (navegador), ou em [`SEM_LUGAR`], que é
    /// vazia.
    #[test]
    fn toda_bancada_da_pasta_roda_em_algum_lugar() {
        let do_job = bancadas_dos_jobs_de_navegador();
        let pasta = raiz().join("apps/seele-app/bancada");
        let mut sem_lugar: Vec<String> = std::fs::read_dir(&pasta)
            .expect("a pasta das bancadas é legível")
            .map(|entrada| {
                entrada
                    .expect("cada entrada da pasta das bancadas é legível")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|nome| {
                std::path::Path::new(nome)
                    .extension()
                    .is_some_and(|extensao| extensao == "cjs")
            })
            .filter(|nome| !NAO_SAO_BANCADA.contains(&nome.as_str()))
            .filter(|nome| {
                !BANCADAS.contains(&nome.as_str())
                    && !do_job.contains(nome)
                    && !SEM_LUGAR.contains(&nome.as_str())
            })
            .collect();
        sem_lugar.sort();
        assert!(
            sem_lugar.is_empty(),
            "bancada que não roda em lugar nenhum: {sem_lugar:?}. Uma prova que ninguém roda \
             fica verde para sempre: a de Node puro entra em `BANCADAS` (xtask/src/\
             check_runtime.rs), a de navegador num `run: node apps/seele-app/bancada/<nome>` do \
             job `bancadas` do `ci.yml` (ou do `bancadas-de-mod`, se ela precisa do `.wasm`), e a \
             que não deve rodar entra, por nome e com o porquê, em `SEM_LUGAR` deste teste. Os \
             jobs hoje rodam: {do_job:?}"
        );
    }

    /// **Estar no job não é rodar: um passo de bancada roda depois de uma
    /// vermelha, e tem prazo.**
    ///
    /// No Actions, um passo que falha pula os seguintes, e um passo pulado não
    /// fica nem verde nem vermelho. Até 01/10/2026 a `ajustes-v013.cjs` ficava
    /// vermelha por um retrato de mentira velho, e por causa dela a
    /// `escolha-de-versao-de-mod.cjs` e a `gif-na-conversa.cjs`, que vinham
    /// depois, eram puladas em todo disparo, enquanto o teste acima dizia que
    /// rodavam. Com `if: ${{ !cancelled() }}` cada passo roda mesmo depois de
    /// uma vermelha, e com `timeout-minutes` uma bancada que pendura não segura
    /// o job até o teto de 360 minutos, que pularia as seguintes do mesmo jeito.
    /// Vale nos dois jobs de navegador.
    #[test]
    fn todo_passo_de_bancada_do_job_de_navegador_roda_depois_de_uma_vermelha_e_tem_prazo() {
        for job in JOBS_COM_BANCADA_DE_NAVEGADOR {
            let passos: Vec<(String, String)> = passos_do_job(job)
                .into_iter()
                .filter_map(|passo| Some((bancada_do_passo(&passo)?, passo)))
                .collect();
            assert!(
                !passos.is_empty(),
                "o job `{job}` do `ci.yml` não tem passo de bancada nenhum que este guarda \
                 reconheça, e ele passaria sem medir nada"
            );
            for (nome, passo) in &passos {
                assert_eq!(
                    chave_do_passo(passo, "if"),
                    Some("${{ !cancelled() }}"),
                    "ci.yml: o passo de {nome}, no job `{job}`, não tem `if: ${{{{ !cancelled() \
                     }}}}`: quando um passo antes dele reprova, o Actions o pula, ele não fica nem \
                     verde nem vermelho, e `toda_bancada_da_pasta_roda_em_algum_lugar` continua \
                     dizendo que ele roda:\n{passo}"
                );
                assert!(
                    chave_do_passo(passo, "timeout-minutes").is_some_and(|prazo| !prazo.is_empty()),
                    "ci.yml: o passo de {nome}, no job `{job}`, não tem prazo \
                     (`timeout-minutes`): uma bancada que pendura segura o job por seis horas, e \
                     as seguintes não rodam:\n{passo}"
                );
            }
        }
    }

    /// **As bancadas de navegador com `.wasm` leem o que o job acabou de
    /// compilar.** Cada uma recebe o `.wasm` pelo caminho: a do executor, o
    /// motor; a da voz e a da identidade, o `seele-web`. Num passo antes do `cargo xtask web`,
    /// ou com outro caminho, ela mediria um `.wasm` velho do cache, ou nenhum.
    #[test]
    fn a_bancada_do_executor_no_navegador_roda_depois_do_motor_compilado() {
        let passos = passos_do_job("bancadas-de-mod");
        let compila = passos
            .iter()
            .position(|passo| chave_do_passo(passo, "run") == Some("cargo xtask web"))
            .expect("o job `bancadas-de-mod` roda `cargo xtask web`");
        for (bancada, wasm) in [
            (
                "executor-no-navegador.cjs",
                "target/wasm32-wasip1/release/seele_motor_web.wasm",
            ),
            (
                "voz-no-navegador.cjs",
                "target/wasm32-unknown-unknown/release/seele_web.wasm",
            ),
            (
                "identidade-no-navegador.cjs",
                "target/wasm32-unknown-unknown/release/seele_web.wasm",
            ),
        ] {
            let roda = passos
                .iter()
                .position(|passo| bancada_do_passo(passo).as_deref() == Some(bancada))
                .unwrap_or_else(|| panic!("o job `bancadas-de-mod` roda a `{bancada}`"));
            assert!(
                compila < roda,
                "ci.yml: a `{bancada}` roda antes do `cargo xtask web`, e mede o `.wasm` que \
                 estiver no cache"
            );
            assert_eq!(
                passos
                    .get(roda)
                    .and_then(|passo| chave_do_passo(passo, "run")),
                Some(format!("node apps/seele-app/bancada/{bancada} {wasm}").as_str()),
                "ci.yml: a `{bancada}` tem de receber o `.wasm` que o `cargo xtask web` compila"
            );
            assert!(
                ler("xtask/src/web.rs").contains(&format!("\"{wasm}\"")),
                "o `cargo xtask web` deixou de compilar {wasm}, e a bancada do CI lê esse caminho"
            );
        }
    }

    /// O comando que roda os roteiros da casca, nos dois lugares que os rodam:
    /// a carga, com um `--roteiro tools/roteiros/<nome>` por roteiro.
    const CARGA_DA_CASCA: [&str; 2] = ["python3", "tools/carga-da-casca.py"];

    /// Os roteiros de `tools/roteiros/`, pelo nome do arquivo, lidos da pasta e
    /// não de um literal: é a pasta que diz quais existem.
    fn roteiros_da_pasta() -> Vec<String> {
        let mut nomes: Vec<String> = std::fs::read_dir(raiz().join("tools/roteiros"))
            .expect("a pasta dos roteiros é legível")
            .map(|entrada| {
                entrada
                    .expect("cada entrada da pasta dos roteiros é legível")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|nome| {
                std::path::Path::new(nome)
                    .extension()
                    .is_some_and(|extensao| extensao == "js")
            })
            .collect();
        nomes.sort();
        nomes
    }

    /// Os roteiros que um comando roda, se ele é a carga: as palavras depois de
    /// [`CARGA_DA_CASCA`] têm de ser pares `--roteiro tools/roteiros/<nome>`.
    /// `None` quando o comando é outro. Uma palavra que não é um desses pares
    /// reprova aqui: um guarda que pulasse o que não entende passaria a contar
    /// um roteiro que a carga não roda, ou a perder um que roda.
    fn roteiros_do_comando(palavras: &[String]) -> Option<Vec<String>> {
        let (comando, resto) = palavras.split_at_checked(CARGA_DA_CASCA.len())?;
        if comando != CARGA_DA_CASCA {
            return None;
        }
        let pares = resto.chunks(2);
        Some(
            pares
                .map(|par| match par {
                    [opcao, caminho] if opcao == "--roteiro" => caminho
                        .strip_prefix("tools/roteiros/")
                        .map(str::to_owned)
                        .unwrap_or_else(|| {
                            panic!("a carga roda um roteiro de fora de tools/roteiros/: {caminho}")
                        }),
                    _ => panic!(
                        "a chamada da carga tem {par:?}, que não é um `--roteiro \
                         tools/roteiros/<nome>`, e este guarda não sabe o que ela roda: \
                         {palavras:?}"
                    ),
                })
                .collect(),
        )
    }

    /// O texto que o shell recebe do `run:` de um passo, como o YAML o entrega:
    /// o valor da linha, ou o bloco escalar. Num `>` ou `>-` o YAML dobra as
    /// linhas do bloco numa só, juntas por espaço, e uma linha em branco vira
    /// quebra; num `|` ele as entrega como estão. Uma linha do bloco com outro
    /// recuo o YAML dobra de outro jeito (ela fica numa linha própria), e aqui
    /// isso reprova em vez de adivinhar.
    fn run_do_passo(passo: &str) -> Option<String> {
        let linhas: Vec<&str> = passo.lines().collect();
        let (onde, linha) = linhas.iter().enumerate().find(|(_, linha)| {
            let linha = linha.trim();
            let linha = linha.strip_prefix("- ").unwrap_or(linha).trim_start();
            linha.starts_with("run:")
        })?;
        let valor = chave_do_passo(linha, "run")?;
        if !valor.starts_with(['>', '|']) {
            return Some(valor.to_owned());
        }
        // O recuo da chave, e não o da linha: num `- run: >-` a chave começa
        // depois do `- `.
        let recuo_da_chave = linha.find("run:").unwrap_or_default();
        let mut bloco: Vec<&str> = linhas
            .iter()
            .skip(onde + 1)
            .take_while(|linha| linha.trim().is_empty() || recuo(linha) > recuo_da_chave)
            .copied()
            .collect();
        while bloco.last().is_some_and(|linha| linha.trim().is_empty()) {
            bloco.pop();
        }
        let recuos: std::collections::BTreeSet<usize> = bloco
            .iter()
            .filter(|linha| !linha.trim().is_empty())
            .map(|linha| recuo(linha))
            .collect();
        assert!(
            recuos.len() <= 1,
            "ci.yml: o bloco do `run:` tem linhas com recuos {recuos:?}, e o YAML não dobra \
             uma linha mais recuada junto com as outras:\n{passo}"
        );
        if valor.starts_with('|') {
            return Some(
                bloco
                    .iter()
                    .map(|linha| linha.trim())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        let mut dobrado = String::new();
        let mut depois_de_texto = false;
        for linha in bloco {
            if linha.trim().is_empty() {
                dobrado.push('\n');
                depois_de_texto = false;
            } else {
                if depois_de_texto {
                    dobrado.push(' ');
                }
                dobrado.push_str(linha.trim());
                depois_de_texto = true;
            }
        }
        Some(dobrado)
    }

    /// Os passos do job `bancadas` que rodam a carga, com os roteiros de cada
    /// um, e a posição de cada passo no job. O `run:` é lido como o YAML o
    /// entrega e como o shell o lê: o que um `#` corta não roda, e não conta.
    fn passos_dos_roteiros_no_ci() -> Vec<(usize, String, Vec<String>)> {
        passos_do_job_como_o_yaml_os_entrega("bancadas")
            .into_iter()
            .enumerate()
            .filter_map(|(onde, passo)| {
                let comandos = comandos_do_shell(&run_do_passo(&passo)?);
                let das_cargas: Vec<Vec<String>> = comandos
                    .iter()
                    .filter_map(|comando| roteiros_do_comando(comando))
                    .collect();
                if das_cargas.is_empty() {
                    return None;
                }
                // **A carga, e nada antes nem depois dela.** Um `exit 0` numa
                // linha própria antes da chamada deixava o guarda verde e o
                // Actions sem roteiro nenhum (R2-M1 da revisão do lote RT).
                assert_eq!(
                    comandos.len(),
                    1,
                    "ci.yml: o `run:` do passo dos roteiros tem de ser só a chamada da carga, e \
                     tem {} comandos: {comandos:?}",
                    comandos.len()
                );
                Some((onde, passo, das_cargas.into_iter().flatten().collect()))
            })
            .collect()
    }

    /// Os comandos de um texto de shell, cada um com as suas palavras, lidos
    /// como o shell os lê no que importa a este guarda:
    ///
    /// - a quebra de linha separa comandos, menos logo depois de uma `\` (a
    ///   continuação);
    /// - fora de aspas, a `\` escapa o caractere seguinte: `\ ` é um espaço
    ///   dentro de uma palavra, e **não** continuação, mesmo no fim da linha;
    /// - um `#` no começo de uma palavra, fora de aspas, comenta até o fim da
    ///   linha, inclusive uma `\` que venha depois dele;
    /// - as aspas juntam, e dentro das duplas a `\` escapa o seguinte.
    ///
    /// `;`, `|` e `&` ficam como letras: uma chamada com `|| true` no fim ganha
    /// palavras que a leitura da carga não aceita, e reprova. Não é um shell.
    fn comandos_do_shell(texto: &str) -> Vec<Vec<String>> {
        let mut comandos = Vec::new();
        let mut palavras: Vec<String> = Vec::new();
        let mut atual = String::new();
        let mut tem_palavra = false;
        let mut aspas: Option<char> = None;
        let mut letras = texto.chars().peekable();
        while let Some(letra) = letras.next() {
            match aspas {
                Some(fecha) if letra == fecha => aspas = None,
                Some('"') if letra == '\\' => match letras.next() {
                    Some('\n') | None => {}
                    Some(seguinte) => atual.push(seguinte),
                },
                Some(_) => atual.push(letra),
                None => match letra {
                    '\\' => match letras.next() {
                        Some('\n') | None => {}
                        Some(seguinte) => {
                            atual.push(seguinte);
                            tem_palavra = true;
                        }
                    },
                    '"' | '\'' => {
                        aspas = Some(letra);
                        tem_palavra = true;
                    }
                    '#' if !tem_palavra => {
                        while letras.next_if(|seguinte| *seguinte != '\n').is_some() {}
                    }
                    '\n' => {
                        if tem_palavra {
                            palavras.push(std::mem::take(&mut atual));
                            tem_palavra = false;
                        }
                        if !palavras.is_empty() {
                            comandos.push(std::mem::take(&mut palavras));
                        }
                    }
                    espaco if espaco.is_whitespace() => {
                        if tem_palavra {
                            palavras.push(std::mem::take(&mut atual));
                            tem_palavra = false;
                        }
                    }
                    outra => {
                        atual.push(outra);
                        tem_palavra = true;
                    }
                },
            }
        }
        if tem_palavra {
            palavras.push(atual);
        }
        if !palavras.is_empty() {
            comandos.push(palavras);
        }
        comandos
    }

    /// Os comandos do corpo de uma função do `publicar.sh`, lidos como o shell
    /// os lê ([`comandos_do_shell`]) sobre o texto **com** os comentários: da
    /// linha `<nome>() {` até a primeira `}` sozinha na margem. Tirar o
    /// comentário linha a linha antes de juntar as continuações errava dois
    /// casos que o shell não continua: `\  # nota` e um espaço depois da `\`
    /// (I1 da revisão do lote RT, medido com `sh`).
    fn comandos_da_funcao(nome: &str) -> Vec<Vec<String>> {
        let texto = ler("empacotar/publicar.sh");
        let linhas: Vec<&str> = texto.lines().collect();
        let abre = format!("{nome}() {{");
        let inicio = linhas
            .iter()
            .position(|linha| linha.trim_end() == abre)
            .unwrap_or_else(|| panic!("o `publicar.sh` não tem mais a função `{nome}`"));
        let corpo: Vec<&str> = linhas
            .iter()
            .skip(inicio + 1)
            .take_while(|linha| linha.trim_end() != "}")
            .copied()
            .collect();
        comandos_do_shell(&corpo.join("\n"))
    }

    /// Os roteiros que a bateria do `publicar.sh` roda: a chamada de
    /// `etapa_da_bateria` em `roteiros_da_casca` cujo comando — depois do
    /// `--prazo <função>`, que é opcional, da queixa e do conserto — é a carga.
    /// Exige também que a `bateria()` chame a etapa, num comando só dela: uma
    /// função que ninguém chama não roda.
    fn roteiros_no_publicar() -> Vec<String> {
        assert!(
            comandos_da_funcao("bateria")
                .iter()
                .any(|comando| comando.as_slice() == ["roteiros_da_casca"]),
            "a `bateria()` do `publicar.sh` não chama mais `roteiros_da_casca`, e uma versão \
             volta a sair com um roteiro vermelho"
        );
        // **A etapa, e nada antes nem depois dela**, como no passo do CI: um
        // `return 0` antes da chamada deixava este guarda verde (R2-M1; a
        // bancada do `publicar.sh` também o pega, por comportamento).
        let comandos = comandos_da_funcao("roteiros_da_casca");
        assert_eq!(
            comandos.len(),
            1,
            "a `roteiros_da_casca` do `publicar.sh` tem de ser só a chamada da etapa, e tem {} \
             comandos: {comandos:?}",
            comandos.len()
        );
        let chamadas: Vec<Vec<String>> = comandos
            .into_iter()
            .filter(|palavras| {
                palavras
                    .first()
                    .is_some_and(|primeira| primeira == "etapa_da_bateria")
            })
            .filter_map(|palavras| {
                let argumentos = palavras.get(1..).unwrap_or_default();
                let argumentos = match argumentos {
                    [opcao, _, resto @ ..] if opcao == "--prazo" => resto,
                    _ => argumentos,
                };
                roteiros_do_comando(argumentos.get(2..).unwrap_or_default())
            })
            .collect();
        assert_eq!(
            chamadas.len(),
            1,
            "a `roteiros_da_casca` do `publicar.sh` tem de rodar a carga por uma \
             `etapa_da_bateria` só, e roda por {}",
            chamadas.len()
        );
        chamadas.into_iter().flatten().collect()
    }

    /// O que cada lista deixa de fora da pasta, e o que ela nomeia e a pasta
    /// não tem.
    fn diferenca(lista: &[String], pasta: &[String]) -> (Vec<String>, Vec<String>) {
        let faltam = pasta
            .iter()
            .filter(|nome| !lista.contains(nome))
            .cloned()
            .collect();
        let sobram = lista
            .iter()
            .filter(|nome| !pasta.contains(nome))
            .cloned()
            .collect();
        (faltam, sobram)
    }

    /// **Todo roteiro da pasta roda no CI e na bateria do `publicar.sh`.**
    ///
    /// Os roteiros de `tools/roteiros/` só rodavam na bateria da
    /// orquestradora, e assim apodreciam calados: o `aceite-de-mods.js` ficou
    /// três semanas sem provar nada e quebrou duas vezes (lote W3 da F5). O
    /// lote RT os pôs, por nome, no job `bancadas` do `ci.yml` e na bateria do
    /// `publicar.sh`, por onde as versões saem. Este teste é a conta: cada `.js`
    /// da pasta é um `--roteiro` da chamada da carga nos dois lugares, e
    /// nenhuma das duas nomeia arquivo que a pasta não tem.
    #[test]
    fn todo_roteiro_da_pasta_roda_no_ci_e_no_publicar() {
        let pasta = roteiros_da_pasta();
        assert!(
            !pasta.is_empty(),
            "não achei roteiro nenhum em tools/roteiros/, e este guarda passaria sem medir nada"
        );

        let no_ci: Vec<String> = passos_dos_roteiros_no_ci()
            .into_iter()
            .flat_map(|(_, _, roteiros)| roteiros)
            .collect();
        let (faltam, sobram) = diferenca(&no_ci, &pasta);
        assert!(
            faltam.is_empty() && sobram.is_empty(),
            "ci.yml: o job `bancadas` não roda estes roteiros da pasta: {faltam:?}; e nomeia \
             estes, que a pasta não tem: {sobram:?}. Um roteiro que não roda fica verde para \
             sempre: ele entra como `--roteiro tools/roteiros/<nome>` na chamada da \
             `python3 tools/carga-da-casca.py` do passo «os roteiros da casca»"
        );

        let no_publicar = roteiros_no_publicar();
        let (faltam, sobram) = diferenca(&no_publicar, &pasta);
        assert!(
            faltam.is_empty() && sobram.is_empty(),
            "empacotar/publicar.sh: a bateria não roda estes roteiros da pasta: {faltam:?}; e \
             nomeia estes, que a pasta não tem: {sobram:?}. Uma versão sai com um roteiro \
             vermelho sem ninguém ver: ele entra como `--roteiro tools/roteiros/<nome>` na \
             chamada da carga em `roteiros_da_casca`"
        );
    }

    /// **O passo dos roteiros roda depois de uma vermelha, tem prazo, e vem
    /// depois do navegador.** O `if` e o prazo pelas mesmas razões dos passos
    /// de bancada (ver o teste acima deles). E o navegador: a carga procura o
    /// `chrome-headless-shell` no cache do Playwright, e é o `npx playwright
    /// install` do job que o põe lá; antes dele, a carga sai com 2 dizendo que
    /// não achou navegador nenhum.
    #[test]
    fn o_passo_dos_roteiros_roda_depois_de_uma_vermelha_tem_prazo_e_vem_depois_do_navegador() {
        let passos = passos_do_job("bancadas");
        let instala = passos
            .iter()
            .position(|passo| {
                chave_do_passo(passo, "run") == Some("npx playwright install --with-deps chromium")
            })
            .expect("o job `bancadas` instala o Chromium do Playwright");
        let dos_roteiros = passos_dos_roteiros_no_ci();
        assert_eq!(
            dos_roteiros.len(),
            1,
            "ci.yml: o job `bancadas` tem de chamar a carga dos roteiros num passo só, e tem \
             {}",
            dos_roteiros.len()
        );
        for (onde, passo, _) in &dos_roteiros {
            assert!(
                instala < *onde,
                "ci.yml: os roteiros rodam antes de o Chromium ser instalado:\n{passo}"
            );
            assert_eq!(
                chave_do_passo(passo, "if"),
                Some("${{ !cancelled() }}"),
                "ci.yml: o passo dos roteiros não tem `if: ${{{{ !cancelled() }}}}`, e uma \
                 bancada vermelha antes dele o pula:\n{passo}"
            );
            assert!(
                chave_do_passo(passo, "timeout-minutes").is_some_and(|prazo| !prazo.is_empty()),
                "ci.yml: o passo dos roteiros não tem prazo (`timeout-minutes`), e um roteiro \
                 que pendura segura o job por seis horas:\n{passo}"
            );
        }
    }

    /// **Os consertos da carga da casca têm guarda que roda em todo `cargo
    /// test`, sem navegador** (M3 da revisão do lote RT).
    ///
    /// Os dois consertos da carga — o relato que atravessa a aspa e os vários
    /// `--roteiro` numa chamada — só se provavam por reversão à mão. Revertido o
    /// segundo, o CI voltaria a rodar só o último roteiro, verde. O autoteste da
    /// carga (`--autoteste`) exercita, sem abrir navegador, o mesmo código que a
    /// carga usa: a leitura do relato sobre uma linha do console no formato do
    /// Chromium, a leitura dos argumentos, a lista que roda todos e diz quem
    /// reprovou, a carga que não cai, e o prazo e o SIGTERM que matam o grupo do
    /// navegador.
    ///
    /// Aqui, e não num passo do job de navegador: o `cargo test` roda em todo
    /// disparo do CI, no `validar` do `release.yml` e na bateria do
    /// `publicar.sh`, e o autoteste leva cerca de um segundo. Só em Unix, que é
    /// onde a carga roda (o `chrome-headless-shell` que ela procura mora nos
    /// caches do Playwright do macOS e do Linux), e o prazo dela mata um grupo
    /// de processos, que é coisa de Unix.
    #[cfg(unix)]
    #[test]
    fn a_carga_da_casca_passa_no_proprio_autoteste() {
        let saida = match std::process::Command::new("python3")
            .arg(raiz().join("tools/carga-da-casca.py"))
            .arg("--autoteste")
            .output()
        {
            Ok(saida) => saida,
            Err(erro) => {
                eprintln!(
                    "PARCIAL: o python3 não executa aqui ({erro}), e o autoteste da carga da \
                     casca não rodou"
                );
                return;
            }
        };
        let dito = format!(
            "{}{}",
            String::from_utf8_lossy(&saida.stdout),
            String::from_utf8_lossy(&saida.stderr)
        );
        assert!(
            saida.status.success(),
            "o autoteste da carga da casca reprovou, e um dos consertos dela voltou atrás:\n{dito}"
        );
        assert!(
            dito.contains("autoteste: os 6 passaram"),
            "o autoteste da carga saiu com 0 sem dizer que os seis passaram:\n{dito}"
        );
    }

    /// **O leitor de shell do guarda lê como o `sh` leu.** Os casos são os que o
    /// `sh` do macOS respondeu numa sonda (lote RT, rodada 2): uma nota depois
    /// de `\ ` e um espaço depois da `\` não continuam a linha, e a `\` no fim
    /// continua. E um `#` só comenta no começo de uma palavra, fora de aspas.
    #[test]
    fn o_leitor_de_shell_do_guarda_le_como_o_sh() {
        let de = |linhas: &[&[&str]]| -> Vec<Vec<String>> {
            linhas
                .iter()
                .map(|comando| {
                    comando
                        .iter()
                        .map(|palavra| (*palavra).to_owned())
                        .collect()
                })
                .collect()
        };
        assert_eq!(
            comandos_do_shell("f a \\  # nota\n  b c\n"),
            de(&[&["f", "a", " "], &["b", "c"]]),
            "uma nota depois de `\\ ` comenta, e a linha não continua"
        );
        assert_eq!(
            comandos_do_shell("f a \\ \n  b c\n"),
            de(&[&["f", "a", " "], &["b", "c"]]),
            "um espaço depois da `\\` é escapado, e a linha não continua"
        );
        assert_eq!(
            comandos_do_shell("f a \\\n  b c\n"),
            de(&[&["f", "a", "b", "c"]]),
            "a `\\` no fim da linha continua"
        );
        assert_eq!(
            comandos_do_shell("f a # x \\\n b\n"),
            de(&[&["f", "a"], &["b"]]),
            "a `\\` dentro de um comentário não continua"
        );
        assert_eq!(
            comandos_do_shell("f '#' a#b \"c # d\" --x  # nota --y\n"),
            de(&[&["f", "#", "a#b", "c # d", "--x"]]),
            "o `#` só comenta no começo de uma palavra e fora de aspas"
        );
    }

    /// O que a lista escreve, num diário que o `node` de mentira também usa:
    /// é a ordem entre os dois que o teste mede.
    struct Diario<'a>(&'a std::cell::RefCell<String>);

    impl std::io::Write for Diario<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .borrow_mut()
                .push_str(&String::from_utf8_lossy(bytes));
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// **Cada bancada é dita antes de rodar.**
    ///
    /// Uma bancada que pendura de verdade — um temporizador vivo, um laço —
    /// não reprova: ela segura o passo até o prazo matá-lo, e a lista, que só
    /// nomeava a que reprovava, deixava quem lê o registro deduzir a culpada
    /// pela ordem das linhas de sucesso. Dita antes, a última linha da lista é
    /// o nome dela. E a que reprova não para as seguintes.
    #[test]
    fn cada_bancada_e_dita_antes_de_rodar() {
        let diario = std::cell::RefCell::new(String::new());
        let reprovada = "regiao-do-mod.cjs";
        let passou = rodar(&raiz(), &BANCADAS, &mut Diario(&diario), |bancada| {
            let nome = bancada
                .file_name()
                .map(|nome| nome.to_string_lossy().into_owned())
                .unwrap_or_default();
            diario
                .borrow_mut()
                .push_str(&format!("[o node rodou {nome}]\n"));
            Ok((nome == reprovada).then(|| "exit status: 1".to_owned()))
        });
        let esperado: String = BANCADAS
            .iter()
            .map(|nome| {
                let reprovou = if *nome == reprovada {
                    format!("check-runtime: {nome} reprovou (exit status: 1)\n")
                } else {
                    String::new()
                };
                format!("check-runtime: rodando {nome}\n[o node rodou {nome}]\n{reprovou}")
            })
            .collect();
        assert_eq!(
            diario.into_inner(),
            esperado,
            "a lista deixou de dizer cada bancada antes de rodá-la: uma que pendura vira um \
             passo morto pelo prazo sem nome, e quem lê o registro tem de deduzir a culpada"
        );
        assert!(
            !passou,
            "uma bancada reprovou e a lista respondeu que passou"
        );
    }

    #[test]
    fn o_passo_das_bancadas_tem_prazo_e_no_validar_vem_depois_dos_testes_sem_ser_pulado() {
        // Uma bancada que pendura de verdade — um temporizador vivo, um laço —
        // não sai nem por `exit` nem por `beforeExit`, e sem prazo o passo
        // seguraria o job até o teto, 360 minutos.
        for caminho in [".github/workflows/release.yml", ".github/workflows/ci.yml"] {
            let (_, passo) = passo_da_lista(caminho);
            assert!(
                passo.contains("timeout-minutes:"),
                "{caminho}: o passo das bancadas perdeu o prazo, e uma que pendura segura o \
                 job por seis horas:\n{passo}"
            );
        }

        // No `validar`, uma bancada vermelha não pode pular o clippy e os
        // testes, e um clippy ou um teste vermelho não pode pular as bancadas.
        let (linha_da_lista, passo) = passo_da_lista(".github/workflows/release.yml");
        assert!(
            passo.contains("!cancelled()"),
            "release.yml: o passo das bancadas voltou a ser pulado quando um passo de antes \
             reprova:\n{passo}"
        );
        let texto = ler(".github/workflows/release.yml");
        let linha_dos_testes = sem_comentario(&texto)
            .iter()
            .position(|linha| linha.trim() == "- name: Testes")
            .expect("o `validar` tem o passo «Testes»");
        assert!(
            linha_dos_testes < linha_da_lista,
            "release.yml: as bancadas voltaram para antes dos testes, e uma vermelha os pula"
        );
    }
}
