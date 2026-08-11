//! Os Dogmas onde este cliente já esteve.
//!
//! Existe para que ninguém precise redigitar um endereço IP e um apelido toda
//! vez. É o que alimenta a tela de conexão do `plug` e a lista do app.
//!
//! # Por que não junto dos pins
//!
//! O arquivo de pins guarda `host` e impressão digital, e é a coisa mais
//! sensível que o cliente escreve em disco: é ele que decide se um servidor é
//! o mesmo de ontem. Formato de uma linha, duas colunas, legível a olho — de
//! propósito, porque quem foi avisado de que a chave mudou precisa abrir e
//! comparar.
//!
//! Acrescentar apelido e último Cage ali dentro tornaria esse arquivo maior,
//! mais fácil de corromper, e menos óbvio de ler. Conveniência e segurança em
//! arquivos separados: um pode ser apagado sem consequência, o outro não.
//!
//! # Formato
//!
//! Uma linha por Dogma, campos separados por tabulação:
//!
//! ```text
//! 192.168.0.7:8383 <TAB> ayanami <TAB> 1 <TAB> 1738000000
//! ```
//!
//! endereço, apelido, último Cage, e quando foi a última visita. Texto porque
//! alguém vai querer editar isso à mão, e binário transformaria uma limpeza de
//! lista numa conversa de suporte.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Um Dogma que este cliente já visitou.
///
/// `Serialize` pelo mesmo motivo de [`crate::search::Match`]: a casca desktop
/// manda esta lista para a webview, e nada aqui é segredo — endereço, apelido e
/// data são exatamente o que a pessoa digitou e o que ela precisa ver de volta
/// para escolher para onde voltar.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Conhecido {
    /// `host` ou `host:porta`, como foi digitado.
    pub alvo: String,
    /// Com que apelido se entrou da última vez.
    pub apelido: String,
    /// Último Cage em que o plug foi inserido.
    pub cage: Option<u32>,
    /// Quando foi a última visita, em segundos desde a época.
    pub visto_em: i64,
}

/// A lista, em disco.
#[derive(Debug, Clone, Default)]
pub struct Conhecidos {
    caminho: PathBuf,
    entradas: Vec<Conhecido>,
}

impl Conhecidos {
    /// Lê a lista, ou começa uma vazia.
    ///
    /// Uma linha ilegível é pulada em vez de derrubar a leitura: isto é
    /// conveniência, e recusar abrir o cliente por causa de uma lista de
    /// atalhos corrompida seria a troca errada.
    ///
    /// # Errors
    ///
    /// Falha só se o diretório não puder ser criado.
    pub fn abrir(caminho: PathBuf) -> Result<Self> {
        if let Some(pai) = caminho.parent() {
            std::fs::create_dir_all(pai)
                .with_context(|| format!("não consegui criar {}", pai.display()))?;
        }

        let entradas = std::fs::read_to_string(&caminho)
            .map(|texto| texto.lines().filter_map(analisar_linha).collect())
            .unwrap_or_default();

        Ok(Self { caminho, entradas })
    }

    /// Os Dogmas conhecidos, do mais recente para o mais antigo.
    ///
    /// Essa ordem é a única útil numa lista de atalhos: quem vai voltar,
    /// volta para onde esteve por último.
    #[must_use]
    pub fn listar(&self) -> Vec<Conhecido> {
        let mut lista = self.entradas.clone();
        lista.sort_by_key(|entrada| std::cmp::Reverse(entrada.visto_em));
        lista
    }

    /// O que se sabe sobre um endereço.
    #[must_use]
    pub fn buscar(&self, alvo: &str) -> Option<&Conhecido> {
        self.entradas.iter().find(|e| e.alvo == alvo)
    }

    /// Registra uma visita, substituindo o que havia.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn registrar(&mut self, alvo: &str, apelido: &str, cage: Option<u32>) -> Result<()> {
        let agora = agora_em_segundos();
        // Sem tabulação nem quebra de linha nos campos, ou a próxima leitura
        // entende um registro como dois.
        let alvo = higienizar(alvo);
        let apelido = higienizar(apelido);

        self.entradas.retain(|e| e.alvo != alvo);
        self.entradas.push(Conhecido {
            alvo,
            apelido,
            cage,
            visto_em: agora,
        });
        self.gravar()
    }

    /// Esquece um Dogma.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn esquecer(&mut self, alvo: &str) -> Result<()> {
        self.entradas.retain(|e| e.alvo != alvo);
        self.gravar()
    }

    fn gravar(&self) -> Result<()> {
        let texto: String = self
            .entradas
            .iter()
            .map(|e| {
                format!(
                    "{}\t{}\t{}\t{}\n",
                    e.alvo,
                    e.apelido,
                    e.cage.map_or_else(|| "-".to_owned(), |c| c.to_string()),
                    e.visto_em
                )
            })
            .collect();
        escrever_privado(&self.caminho, texto.as_bytes())
            .with_context(|| format!("não consegui gravar {}", self.caminho.display()))
    }
}

fn analisar_linha(linha: &str) -> Option<Conhecido> {
    let mut campos = linha.split('\t');
    let alvo = campos.next()?.trim();
    if alvo.is_empty() {
        return None;
    }
    let apelido = campos.next().unwrap_or("").trim().to_owned();
    let cage = campos.next().and_then(|c| c.trim().parse().ok());
    let visto_em = campos
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);

    Some(Conhecido {
        alvo: alvo.to_owned(),
        apelido,
        cage,
        visto_em,
    })
}

/// Tira o que quebraria o formato.
fn higienizar(valor: &str) -> String {
    valor
        .chars()
        .filter(|c| *c != '\t' && *c != '\n' && *c != '\r')
        .collect()
}

/// Mesmo modo restrito da identidade: a lista diz com quem você conversa.
#[cfg(unix)]
fn escrever_privado(caminho: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut arquivo = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(caminho)?;
    arquivo.write_all(bytes)
}

#[cfg(not(unix))]
fn escrever_privado(caminho: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(caminho, bytes)
}

fn agora_em_segundos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |passou| i64::try_from(passou.as_secs()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rascunho(nome: &str) -> PathBuf {
        let mut caminho = std::env::temp_dir();
        caminho.push(format!("seele-conhecidos-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&caminho);
        caminho.join("conhecidos")
    }

    #[test]
    fn uma_visita_sobrevive_ao_processo() {
        let caminho = rascunho("sobrevive");
        {
            let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");
            lista
                .registrar("192.168.0.7:8383", "ayanami", Some(1))
                .expect("registrar");
        }

        let lista = Conhecidos::abrir(caminho.clone()).expect("reabrir");
        let encontrado = lista.buscar("192.168.0.7:8383").expect("achar");
        assert_eq!(encontrado.apelido, "ayanami");
        assert_eq!(encontrado.cage, Some(1));

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    #[test]
    fn visitar_de_novo_atualiza_em_vez_de_duplicar() {
        // Uma lista de atalhos com o mesmo Dogma três vezes é pior que nenhuma.
        let caminho = rascunho("atualiza");
        let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");

        lista
            .registrar("host:8383", "ayanami", Some(1))
            .expect("um");
        lista.registrar("host:8383", "rei", Some(2)).expect("dois");

        assert_eq!(lista.listar().len(), 1);
        let encontrado = lista.buscar("host:8383").expect("achar");
        assert_eq!(encontrado.apelido, "rei");
        assert_eq!(encontrado.cage, Some(2));

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    #[test]
    fn a_lista_vem_do_mais_recente_para_o_mais_antigo() {
        // Quem volta, volta para onde esteve por último.
        let caminho = rascunho("ordem");
        let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");

        lista.registrar("antigo:8383", "a", None).expect("um");
        // O relógio tem segundos de resolução, então o segundo registro é
        // envelhecido à mão para o teste não depender de esperar um segundo.
        lista.entradas[0].visto_em -= 100;
        lista.registrar("recente:8383", "b", None).expect("dois");

        let ordenada = lista.listar();
        assert_eq!(ordenada[0].alvo, "recente:8383");
        assert_eq!(ordenada[1].alvo, "antigo:8383");

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    #[test]
    fn tabulacao_no_apelido_nao_quebra_o_arquivo() {
        // Um apelido com tabulação viraria dois campos, e a leitura seguinte
        // entenderia um registro como outro.
        let caminho = rascunho("higiene");
        {
            let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");
            lista
                .registrar("host:8383", "aya\tnami\nrei", Some(1))
                .expect("registrar");
        }

        let lista = Conhecidos::abrir(caminho.clone()).expect("reabrir");
        assert_eq!(lista.listar().len(), 1);
        assert_eq!(
            lista.buscar("host:8383").expect("achar").apelido,
            "ayanamirei"
        );

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    #[test]
    fn uma_linha_corrompida_e_pulada_e_nao_derruba_o_cliente() {
        // Isto é conveniência. Recusar abrir por causa de uma lista de atalhos
        // ilegível seria a troca errada.
        let caminho = rascunho("corrompida");
        std::fs::create_dir_all(caminho.parent().expect("pai")).expect("mkdir");
        std::fs::write(
            &caminho,
            "\n\nhost:8383\tayanami\t1\t100\nlixo sem tabulação\n",
        )
        .expect("escrever");

        let lista = Conhecidos::abrir(caminho.clone()).expect("abrir");
        assert!(lista.buscar("host:8383").is_some());

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    #[test]
    fn esquecer_tira_da_lista() {
        let caminho = rascunho("esquecer");
        let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");
        lista.registrar("host:8383", "ayanami", None).expect("um");
        lista.esquecer("host:8383").expect("esquecer");

        assert!(lista.listar().is_empty());

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    #[cfg(unix)]
    #[test]
    fn o_arquivo_nao_e_legivel_por_outros() {
        // A lista diz com quem você conversa. Mesmo cuidado da identidade.
        use std::os::unix::fs::PermissionsExt;

        let caminho = rascunho("modo");
        let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");
        lista.registrar("host:8383", "ayanami", None).expect("um");

        let modo = std::fs::metadata(&caminho)
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(modo & 0o077, 0, "outros conseguem ler a lista");

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }
}
