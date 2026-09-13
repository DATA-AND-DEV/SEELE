//! Os conjuntos de MODs que esta máquina já aceitou, por servidor.
//!
//! ADR 0045: «aceitou uma vez, entra direto nas próximas. Um servidor que troca
//! de MOD pergunta de novo.» Este arquivo é a primeira metade dessa frase; a
//! segunda é de graça, porque o que se guarda é a **identidade do conjunto**, e
//! ela muda quando o conjunto muda.
//!
//! # Por que não junto dos pins, nem junto dos conhecidos
//!
//! Pela mesma razão que `conhecidos` não mora no arquivo de pins, e ela está
//! escrita lá: o arquivo de pins decide se um servidor é o mesmo de ontem, e é
//! a coisa mais sensível que o cliente escreve. Apagar este aqui custa uma
//! pergunta a mais na próxima entrada; apagar aquele é outra conversa.
//!
//! Separado de `conhecidos` porque são tempos diferentes: a lista de para onde
//! voltar é conveniência que envelhece — endereço, apelido, última sala —, e um
//! aceite é uma decisão que a pessoa tomou. Misturá-los faria uma limpeza de
//! lista de servidores apagar consentimentos em silêncio.
//!
//! # Formato
//!
//! Uma linha por aceite, campos separados por tabulação:
//!
//! ```text
//! 192.168.0.7:8383 <TAB> 9f86d0…<TAB> 1789092000
//! ```
//!
//! Sob que chave o servidor é arquivado — a mesma `chave_do_pin` —, a
//! identidade do conjunto aceito, e quando. Texto porque alguém vai querer
//! conferir a olho o que aceitou, e porque apagar uma linha à mão tem de ser
//! possível: é assim que uma pessoa desfaz um sim.
//!
//! **Uma linha por servidor**, e não um histórico: aceitar um conjunto novo
//! substitui o anterior. Guardar os antigos seria guardar permissões para
//! conjuntos que o servidor pode voltar a exigir sem a pessoa reler nada.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// O arquivo de aceites de uma instalação.
#[derive(Debug, Clone)]
pub struct Aceites {
    arquivo: PathBuf,
}

/// Um aceite guardado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aceite {
    /// Sob que chave o servidor é arquivado. A mesma do pin.
    pub alvo: String,
    /// A identidade do conjunto aceito, em hexadecimal minúsculo.
    pub conjunto: String,
    /// Quando, em segundos desde a época.
    pub quando: i64,
}

impl Aceites {
    /// Os aceites guardados no diretório de configuração.
    #[must_use]
    pub fn em(diretorio: &Path) -> Self {
        Self {
            arquivo: diretorio.join("aceites"),
        }
    }

    /// Tudo que está guardado.
    ///
    /// Um arquivo que não existe é uma lista vazia — é o estado de toda
    /// instalação que nunca entrou num servidor com MOD. Uma linha estragada é
    /// pulada em vez de derrubar a leitura das outras: uma linha a menos custa
    /// uma pergunta a mais, e a leitura inteira falhando custaria todas.
    #[must_use]
    pub fn listar(&self) -> Vec<Aceite> {
        let Ok(texto) = std::fs::read_to_string(&self.arquivo) else {
            return Vec::new();
        };
        texto.lines().filter_map(ler_linha).collect()
    }

    /// A identidade que esta máquina já aceitou para este servidor, se alguma.
    #[must_use]
    pub fn aceito_de(&self, alvo: &str) -> Option<String> {
        self.listar()
            .into_iter()
            .find(|aceite| aceite.alvo == alvo)
            .map(|aceite| aceite.conjunto)
    }

    /// Guarda o sim que a pessoa acabou de dar.
    ///
    /// Substitui o aceite anterior deste servidor, pela razão do cabeçalho.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn guardar(&self, alvo: &str, conjunto: &str, quando: i64) -> Result<()> {
        let mut linhas: Vec<Aceite> = self
            .listar()
            .into_iter()
            .filter(|aceite| aceite.alvo != alvo)
            .collect();
        linhas.push(Aceite {
            alvo: alvo.to_owned(),
            conjunto: conjunto.to_owned(),
            quando,
        });
        self.escrever(&linhas)
    }

    /// O mesmo, com o relógio desta máquina.
    ///
    /// Existe para que quem chama de fora não precise carregar um relógio, e
    /// [`Self::guardar`] continua recebendo o instante para que os testes não
    /// dependam de um.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn aceitar_agora(&self, alvo: &str, conjunto: &str) -> Result<()> {
        self.guardar(alvo, conjunto, agora_em_segundos())
    }

    /// Desfaz o sim dado a este servidor.
    ///
    /// Existe porque um consentimento que não se retira não é consentimento. A
    /// próxima entrada volta a perguntar.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn esquecer(&self, alvo: &str) -> Result<()> {
        let linhas: Vec<Aceite> = self
            .listar()
            .into_iter()
            .filter(|aceite| aceite.alvo != alvo)
            .collect();
        self.escrever(&linhas)
    }

    fn escrever(&self, linhas: &[Aceite]) -> Result<()> {
        if let Some(pasta) = self.arquivo.parent() {
            std::fs::create_dir_all(pasta)
                .with_context(|| format!("não deu para criar {}", pasta.display()))?;
        }
        let texto: String = linhas
            .iter()
            .map(|aceite| format!("{}\t{}\t{}\n", aceite.alvo, aceite.conjunto, aceite.quando))
            .collect();
        std::fs::write(&self.arquivo, texto)
            .with_context(|| format!("não deu para escrever {}", self.arquivo.display()))
    }
}

fn agora_em_segundos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |passou| i64::try_from(passou.as_secs()).unwrap_or(0))
}

/// Uma linha do arquivo, ou nada.
///
/// A identidade é conferida na leitura, e não só na escrita: o arquivo é texto
/// justamente para ser editável à mão, e um conjunto que não é um hash de
/// conteúdo nunca bateria com anúncio nenhum — melhor não estar na lista do que
/// estar e nunca valer.
fn ler_linha(linha: &str) -> Option<Aceite> {
    let mut campos = linha.split('\t');
    let alvo = campos.next()?.trim();
    let conjunto = campos.next()?.trim();
    let quando = campos
        .next()
        .and_then(|q| q.trim().parse().ok())
        .unwrap_or(0);
    if alvo.is_empty() || !seele_proto::mods::e_hash_de_conteudo(conjunto) {
        return None;
    }
    Some(Aceite {
        alvo: alvo.to_owned(),
        conjunto: conjunto.to_owned(),
        quando,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta(nome: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static QUAL: AtomicUsize = AtomicUsize::new(0);
        let n = QUAL.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("seele-aceites-{nome}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    fn conjunto(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    #[test]
    fn sem_arquivo_nao_ha_aceite_nenhum() {
        let aceites = Aceites::em(&pasta("vazio"));
        assert_eq!(aceites.aceito_de("casa:8383"), None);
    }

    /// «Aceitou uma vez, entra direto nas próximas» — ADR 0045.
    #[test]
    fn o_que_foi_aceito_volta_na_proxima_entrada() {
        let aceites = Aceites::em(&pasta("volta"));
        aceites
            .guardar("casa:8383", &conjunto(0xa1), 1)
            .expect("guardar");
        assert_eq!(aceites.aceito_de("casa:8383"), Some(conjunto(0xa1)));
    }

    /// Um aceite é de um servidor, e não desta máquina: dizer sim para um não
    /// diz nada sobre outro.
    #[test]
    fn um_aceite_nao_vale_para_outro_servidor() {
        let aceites = Aceites::em(&pasta("outro"));
        aceites
            .guardar("casa:8383", &conjunto(0xa1), 1)
            .expect("guardar");
        assert_eq!(aceites.aceito_de("trabalho:8383"), None);
    }

    /// Aceitar um conjunto novo substitui o anterior. Guardar os dois seria
    /// guardar permissão para um conjunto que o servidor pode voltar a exigir
    /// sem a pessoa reler nada.
    #[test]
    fn um_aceite_novo_substitui_o_anterior_do_mesmo_servidor() {
        let aceites = Aceites::em(&pasta("substitui"));
        aceites
            .guardar("casa:8383", &conjunto(0xa1), 1)
            .expect("primeiro");
        aceites
            .guardar("casa:8383", &conjunto(0xb2), 2)
            .expect("segundo");
        assert_eq!(aceites.listar().len(), 1);
        assert_eq!(aceites.aceito_de("casa:8383"), Some(conjunto(0xb2)));
    }

    /// Um consentimento que não se retira não é consentimento.
    #[test]
    fn esquecer_faz_a_proxima_entrada_perguntar_de_novo() {
        let aceites = Aceites::em(&pasta("esquecer"));
        aceites
            .guardar("casa:8383", &conjunto(0xa1), 1)
            .expect("guardar");
        aceites.esquecer("casa:8383").expect("esquecer");
        assert_eq!(aceites.aceito_de("casa:8383"), None);
    }

    /// O arquivo é texto para ser editável à mão, e o que for editado para
    /// dentro dele tem de continuar sendo uma identidade — ou nunca bateria com
    /// anúncio nenhum, e a pessoa acharia que aceitou algo que não vale.
    #[test]
    fn uma_linha_estragada_e_pulada_sem_derrubar_as_outras() {
        let dir = pasta("estragada");
        let aceites = Aceites::em(&dir);
        std::fs::write(
            dir.join("aceites"),
            format!(
                "casa:8383\tnão é hash\t1\nsó um campo\ntrabalho:8383\t{}\t2\n",
                conjunto(0xc3)
            ),
        )
        .expect("escrever à mão");

        assert_eq!(aceites.aceito_de("casa:8383"), None);
        assert_eq!(aceites.aceito_de("trabalho:8383"), Some(conjunto(0xc3)));
    }
}
