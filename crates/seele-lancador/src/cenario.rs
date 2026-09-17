//! Os artefatos de teste deste crate, todos identificados como tal.
//!
//! **Nenhuma versão publicada de verdade é encenada aqui, e nenhuma chave de
//! produção assina nada.** As versões destes cenários levam `-teste` no nome,
//! a chave é derivada de uma semente fixa e o comentário dela diz «nunca de
//! producao» na primeira linha. Quem encontrar um destes artefatos fora de um
//! teste sabe o que ele é sem precisar perguntar.
//!
//! Só há uma exceção, e ela está anotada onde acontece: os identificadores
//! `0.10.5-1`, `0.10.5` e `0.10.4-4` aparecem em
//! [`crate::versao`] e em [`crate::manifesto`] porque a **ordem** entre eles é
//! um fato do `SEELE-RELEASES` que o código precisa respeitar — `0.10.5-1` saiu
//! depois de `0.10.5`, e o semver diria o contrário. Inventar identificadores
//! ali esconderia justamente a armadilha que o teste existe para prender.
//!
//! Sobre a pasta temporária: cada cenário tem a sua, com processo e relógio no
//! nome. Dois testes dividindo uma pasta e apagando o diretório um do outro já
//! custou um conserto neste repositório.

// O módulo inteiro é `#[cfg(test)]`, e `specs/10-convencoes.md` só proíbe
// `unwrap`/`expect` fora de teste: um cenário que falha ao montar tem de
// derrubar o teste ali, com a frase, e não seguir com meio cenário.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use crate::assinatura::fixtures::ChaveDeTeste;
use crate::deposito::{Deposito, Desempacotador};
use crate::revogacao::Vigente;
use crate::{Chave, Manifesto};

/// A semente da chave de teste destes cenários.
const SEMENTE: u8 = 42;

/// O par de chaves de teste, sempre o mesmo.
pub(crate) fn par() -> ChaveDeTeste {
    ChaveDeTeste::com_semente(SEMENTE)
}

/// A chave pública correspondente, para conferir.
pub(crate) fn chave() -> Chave {
    Chave::do_formato_do_atualizador(&par().publica_no_formato_do_atualizador())
        .expect("a chave de teste devia abrir")
}

/// Uma pasta temporária que é só deste cenário.
pub(crate) fn pasta(nome: &str) -> PathBuf {
    let marca = format!(
        "seele-lancador-{nome}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let caminho = std::env::temp_dir().join(marca);
    std::fs::create_dir_all(&caminho).expect("criar a pasta do cenário");
    caminho
}

/// O nome do alvo usado nos cenários. Não é o desta máquina de propósito:
/// nenhum teste deste crate deve depender de onde ele está rodando.
pub(crate) const ALVO: &str = "sistema-de-teste-x86_64";

/// Um pacote de teste, com o conteúdo escrito no próprio nome.
pub(crate) fn pacote(versao: &str) -> Vec<u8> {
    format!("PACOTE-DE-TESTE-DO-SEELE-LANCADOR versao={versao}").into_bytes()
}

/// O caminho do executável que os cenários declaram.
pub(crate) const EXECUTAVEL: &str = "bin/seele-de-teste";

/// Monta o JSON de um manifesto de versões de teste.
///
/// `versoes` vem da mais nova para a mais velha, que é a ordem do contrato.
/// `revogadas` são pares `(versão, defeito, corrigida_em)`.
pub(crate) fn manifesto_json(
    versoes: &[&str],
    revogadas: &[(&str, &str, Option<&str>)],
    emitida_em: &str,
) -> String {
    manifesto_json_com_executavel(versoes, revogadas, emitida_em, Some(EXECUTAVEL))
}

/// O mesmo, com o `executable` escolhido por quem chama.
///
/// `None` é o manifesto **publicado hoje**, que não traz o campo; um texto
/// qualquer é o que um manifesto trocado no caminho poderia trazer — e o
/// manifesto não é assinado.
pub(crate) fn manifesto_json_com_executavel(
    versoes: &[&str],
    revogadas: &[(&str, &str, Option<&str>)],
    emitida_em: &str,
    executavel: Option<&str>,
) -> String {
    let campo_do_executavel = executavel
        .map(|caminho| format!(r#","executable":"{}""#, caminho.replace('\\', "\\\\")))
        .unwrap_or_default();
    let entradas: Vec<String> = versoes
        .iter()
        .map(|v| {
            format!(
                r#"{{"version":"{v}","pub_date":"2026-01-01T00:00:00Z",
                    "unit":{{"protocol":3,"mod_api":1}},
                    "platforms":{{"{ALVO}":{{
                        "url":"https://exemplo.invalido/{v}",
                        "signature":"{}",
                        "sha256":"{}"{campo_do_executavel}}}}}}}"#,
                par().assinar_no_formato_do_atualizador(&pacote(v)),
                crate::deposito::resumo_em_hexadecimal(&pacote(v)),
            )
        })
        .collect();

    let revogacoes = if revogadas.is_empty() {
        String::new()
    } else {
        let itens: Vec<String> = revogadas
            .iter()
            .map(|(alvo, defeito, corrigida)| {
                let corrigida = corrigida
                    .map(|c| format!(r#","fixed_in":"{c}""#))
                    .unwrap_or_default();
                format!(r#"{{"target":"{alvo}","defect":"{defeito}"{corrigida}}}"#)
            })
            .collect();
        let documento = format!(
            r#"{{"schema":1,"issued_at":"{emitida_em}","revoked":[{}]}}"#,
            itens.join(",")
        );
        use base64::Engine as _;
        format!(
            r#","revocations":{{"document":"{}","signature":"{}"}}"#,
            base64::engine::general_purpose::STANDARD.encode(&documento),
            par().assinar_no_formato_do_atualizador(documento.as_bytes())
        )
    };

    format!(
        r#"{{"schema":2,"versions":[{}]{revogacoes}}}"#,
        entradas.join(",")
    )
}

/// Um manifesto de teste, já lido.
pub(crate) fn manifesto(versoes: &[&str], revogadas: &[(&str, &str, Option<&str>)]) -> Manifesto {
    Manifesto::ler(
        manifesto_json(versoes, revogadas, "2026-09-10T00:00:00Z").as_bytes(),
        &chave(),
    )
    .expect("o manifesto de teste devia abrir")
}

/// Um manifesto de teste com o `executable` que se quiser — inclusive nenhum.
pub(crate) fn manifesto_com_executavel(versoes: &[&str], executavel: Option<&str>) -> Manifesto {
    Manifesto::ler(
        manifesto_json_com_executavel(versoes, &[], "2026-09-10T00:00:00Z", executavel).as_bytes(),
        &chave(),
    )
    .expect("o manifesto de teste devia abrir: um `executable` errado não pode derrubá-lo inteiro")
}

/// A lista de revogação que vale para este manifesto neste depósito.
///
/// Passa pela conciliação de verdade, que é o único caminho que produz um
/// [`Vigente`] — e é de propósito: ver [`crate::revogacao::Vigente`]. Um
/// cenário que quer encenar a lista recuando ou sumindo chama
/// [`Deposito::conciliar_revogacoes`] direto, para poder olhar a recusa.
pub(crate) fn vigente(manifesto: &Manifesto, deposito: &Deposito) -> Vigente {
    deposito
        .conciliar_revogacoes(manifesto.envelope_de_revogacoes(), &chave())
        .expect("a lista do cenário devia conciliar")
}

/// Um desempacotador que escreve o executável declarado e um arquivo de marca.
pub(crate) struct DesempacotadorDeTeste;

impl Desempacotador for DesempacotadorDeTeste {
    fn desempacotar(&self, pacote: &[u8], destino: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(destino.join("bin"))?;
        std::fs::write(destino.join(EXECUTAVEL), pacote)?;
        std::fs::write(destino.join("conteudo-de-teste.txt"), pacote)?;
        Ok(())
    }
}

/// Um desempacotador que escreve metade e desiste — para provar que uma falha
/// no meio não alcança o que já estava instalado.
pub(crate) struct DesempacotadorQueFalhaNoMeio;

impl Desempacotador for DesempacotadorQueFalhaNoMeio {
    fn desempacotar(&self, _pacote: &[u8], destino: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(destino.join("bin"))?;
        std::fs::write(destino.join("metade-escrita.txt"), b"nao terminou")?;
        Err(std::io::Error::other(
            "desempacotador de teste desistiu no meio",
        ))
    }
}

/// Um desempacotador que não escreve o executável que o manifesto declara.
pub(crate) struct DesempacotadorSemExecutavel;

impl Desempacotador for DesempacotadorSemExecutavel {
    fn desempacotar(&self, _pacote: &[u8], destino: &Path) -> std::io::Result<()> {
        std::fs::write(destino.join("outra-coisa.txt"), b"nao e o executavel")
    }
}
