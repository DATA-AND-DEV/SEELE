//! Roda as bancadas de MOD que precisam só de Node.
//!
//! Seis, e as seis carregam o arquivo de verdade em vez de uma cópia, num
//! contexto de VM, com o resto de mentira:
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
//!   `src/executor.rs`: o envio de imagem é recusado nas APIs 3 e 4, vai com o
//!   id e a geração de quem hospeda e não com os que o MOD manda, só confirma
//!   depois de gravar, responde `sessao-encerrada` quando o MOD deixa de valer
//!   no meio da gravação, e só existe no SDK com a capacidade `volume`.
//!
//! As de navegador (Playwright) não estão aqui: elas precisam de um Chromium, e
//! só o job `bancadas` do `ci.yml` as roda.
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
//! voltar a nomear uma delas, e se o passo dos workflows perder o prazo ou, no
//! `validar`, voltar para antes dos testes.
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

use std::path::PathBuf;
use std::process::ExitCode;

/// As bancadas, por nome e não por um laço sobre a pasta: um rascunho guardado
/// em `bancada/` não pode passar a reprovar os três lugares que a chamam, e as
/// de navegador moram na mesma pasta.
const BANCADAS: [&str; 6] = [
    "ciclo-do-executor.cjs",
    "regiao-do-mod.cjs",
    "continuacao-de-midia.cjs",
    "contribuicoes-e-camadas.cjs",
    "envio-de-imagens.cjs",
    "imagens-por-volume.cjs",
];

fn raiz() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default()
}

pub(crate) fn run() -> ExitCode {
    let raiz = raiz();
    // **Todas, e não até a primeira que reprova.** Saber que duas quebraram é
    // uma informação diferente de saber que uma quebrou, e é a que diz se a
    // mudança foi num lugar ou na fronteira entre os dois.
    let mut falhou = false;
    for nome in BANCADAS {
        let bancada = raiz.join("apps/seele-app/bancada").join(nome);
        if !bancada.is_file() {
            eprintln!("check-runtime: a bancada sumiu de {}", bancada.display());
            return ExitCode::FAILURE;
        }
        match std::process::Command::new("node").arg(&bancada).status() {
            Ok(estado) if estado.success() => {}
            Ok(estado) => {
                eprintln!("check-runtime: {nome} reprovou ({estado})");
                falhou = true;
            }
            // **Nomeado, e não engolido.** Sem Node não dá para rodar, e dizer
            // «passou» aqui seria a pior resposta possível: quem chamou acharia
            // que as corridas foram provadas.
            Err(erro) => {
                eprintln!("check-runtime: não consegui rodar o `node`: {erro}");
                eprintln!("  a bancada precisa dele, e o produto não — ver o topo de xtask/src/check_runtime.rs");
                return ExitCode::FAILURE;
            }
        }
    }
    if falhou {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
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
