//! Os servidores onde este cliente já esteve.
//!
//! Existe para que ninguém precise redigitar um endereço IP e um apelido toda
//! vez. É o que alimenta a tela de conexão do `connection` e a lista do app.
//!
//! # Por que não junto dos pins
//!
//! O arquivo de pins guarda `host` e impressão digital, e é a coisa mais
//! sensível que o cliente escreve em disco: é ele que decide se um servidor é
//! o mesmo de ontem. Formato de uma linha, duas colunas, legível a olho — de
//! propósito, porque quem foi avisado de que a chave mudou precisa abrir e
//! comparar.
//!
//! Acrescentar apelido e último sala de voz ali dentro tornaria esse arquivo maior,
//! mais fácil de corromper, e menos óbvio de ler. Conveniência e segurança em
//! arquivos separados: um pode ser apagado sem consequência, o outro não.
//!
//! # Formato
//!
//! Uma linha por servidor, campos separados por tabulação:
//!
//! ```text
//! 192.168.0.7:8383 <TAB> ayanami <TAB> 1 <TAB> 1738000000
//! ```
//!
//! endereço, apelido, último sala de voz, e quando foi a última visita. Texto porque
//! alguém vai querer editar isso à mão, e binário transformaria uma limpeza de
//! lista numa conversa de suporte.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Um servidor que este cliente já visitou.
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
    /// Último sala de voz em que o connection foi inserido.
    pub voice_room: Option<u32>,
    /// Quando foi a última visita, em segundos desde a época.
    pub visto_em: i64,
    /// Como o servidor se chamava na última visita.
    ///
    /// `None` numa entrada escrita antes de este campo existir, e a tela cai
    /// para o endereço nesse caso. Guardar o nome é o que faz esta lista servir
    /// a quem não decora IP: «Casa» é o que uma pessoa reconhece, e
    /// `192.168.0.39:8383` é o que ela copiou de alguém uma vez.
    pub nome: Option<String>,
    /// A imagem do servidor na última visita, se havia uma.
    ///
    /// Fora do arquivo de texto: são até 8 KiB de PNG, e binário em coluna de
    /// TSV faria uma limpeza de lista à mão virar conversa de suporte — que é
    /// exatamente o que o cabeçalho deste módulo recusa. Mora num arquivo por
    /// server, ao lado da lista, e vem carregada em [`Conhecidos::listar`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icone: Option<Vec<u8>>,
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

        let entradas: Vec<Conhecido> = std::fs::read_to_string(&caminho)
            .map(|texto| texto.lines().filter_map(analisar_linha).collect())
            .unwrap_or_default();

        let mut lista = Self { caminho, entradas };
        // As imagens vêm do disco aqui, e não a cada `listar`: a lista é
        // desenhada na tela de entrada, que a lê mais de uma vez, e reler
        // alguns arquivos por desenho seria pagar disco por uma decoração.
        //
        // Um arquivo que não abre é ausência de imagem, e não erro: a lista de
        // para-onde-voltar não pode deixar de abrir por causa de um enfeite.
        for indice in 0..lista.entradas.len() {
            let Some(alvo) = lista.entradas.get(indice).map(|e| e.alvo.clone()) else {
                continue;
            };
            let bytes = std::fs::read(lista.caminho_do_icone(&alvo)).ok();
            if let Some(entrada) = lista.entradas.get_mut(indice) {
                entrada.icone = bytes;
            }
        }
        Ok(lista)
    }

    /// Os servidores conhecidos, do mais recente para o mais antigo.
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
    pub fn registrar(&mut self, alvo: &str, apelido: &str, voice_room: Option<u32>) -> Result<()> {
        let agora = agora_em_segundos();
        // Sem tabulação nem quebra de linha nos campos, ou a próxima leitura
        // entende um registro como dois.
        let alvo = higienizar(alvo);
        let apelido = higienizar(apelido);

        // O nome e a imagem que já estavam anotados sobrevivem a uma visita que
        // não os traz: `registrar` reescreve a entrada inteira, e apagá-los aqui
        // faria a lista esquecer a aparência a cada conexão do terminal, que não
        // tem como saber dela.
        let (nome, icone) = self
            .entradas
            .iter()
            .find(|e| e.alvo == alvo)
            .map_or((None, None), |e| (e.nome.clone(), e.icone.clone()));

        self.entradas.retain(|e| e.alvo != alvo);
        self.entradas.push(Conhecido {
            alvo,
            apelido,
            voice_room,
            visto_em: agora,
            nome,
            icone,
        });
        self.gravar()
    }

    /// Anota como o servidor se chama e qual é a imagem dele.
    ///
    /// Separado de [`Self::registrar`] porque acontece noutro momento: o
    /// endereço e o apelido são sabidos quando a conexão dá certo, e estes dois
    /// chegam **depois**, no quadro que o servidor manda logo após o aperto de mão.
    ///
    /// Não cria entrada: só anota sobre uma que já existe. Um servidor que não está
    /// na lista de visitados não está por decisão de quem chamou `registrar` —
    /// um hospedado aqui, por exemplo — e esta função não pode desfazer isso.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn anotar_aparencia(
        &mut self,
        alvo: &str,
        nome: Option<&str>,
        icone: Option<&[u8]>,
    ) -> Result<()> {
        let alvo = higienizar(alvo);
        let Some(entrada) = self.entradas.iter_mut().find(|e| e.alvo == alvo) else {
            return Ok(());
        };
        entrada.nome = nome.map(higienizar);
        entrada.icone = icone.map(<[u8]>::to_vec);
        self.gravar()
    }

    /// Onde a imagem de um servidor é guardada.
    ///
    /// Um arquivo por server, num diretório ao lado da lista. O nome vem do
    /// endereço passado por um digestor, e não do endereço: um `alvo` é texto
    /// que alguém digitou, e texto que alguém digitou não pode virar caminho de
    /// arquivo sem alguém escrever `../` mais cedo ou mais tarde.
    fn caminho_do_icone(&self, alvo: &str) -> PathBuf {
        let mut pasta = self.caminho.clone();
        let nome = pasta.file_name().map_or_else(
            || "conhecidos".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
        pasta.set_file_name(format!("{nome}-icones"));
        pasta.join(format!("{}.png", digerir(alvo)))
    }

    /// Esquece um servidor.
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
                    "{}\t{}\t{}\t{}\t{}\n",
                    e.alvo,
                    e.apelido,
                    e.voice_room
                        .map_or_else(|| "-".to_owned(), |c| c.to_string()),
                    e.visto_em,
                    // Uma coluna a mais no fim, e nunca no meio: uma linha de
                    // quatro campos escrita por uma versão anterior continua
                    // sendo lida, e o campo que falta vira ausência de nome.
                    e.nome.as_deref().unwrap_or("")
                )
            })
            .collect();
        escrever_privado(&self.caminho, texto.as_bytes())
            .with_context(|| format!("não consegui gravar {}", self.caminho.display()))?;

        // As imagens, uma por arquivo. Falhar aqui **não** derruba a gravação da
        // lista: um distintivo que não coube em disco é uma tela sem enfeite, e
        // a lista de para-onde-voltar é a coisa que precisa sobreviver.
        for entrada in &self.entradas {
            let caminho = self.caminho_do_icone(&entrada.alvo);
            match entrada.icone.as_deref() {
                Some(bytes) => {
                    if let Some(pasta) = caminho.parent() {
                        let _ = std::fs::create_dir_all(pasta);
                    }
                    if let Err(erro) = escrever_privado(&caminho, bytes) {
                        tracing::debug!(%erro, caminho = %caminho.display(), "não gravei a imagem");
                    }
                }
                None => {
                    let _ = std::fs::remove_file(&caminho);
                }
            }
        }
        Ok(())
    }
}

fn analisar_linha(linha: &str) -> Option<Conhecido> {
    let mut campos = linha.split('\t');
    let alvo = campos.next()?.trim();
    if alvo.is_empty() {
        return None;
    }
    let apelido = campos.next().unwrap_or("").trim().to_owned();
    let voice_room = campos.next().and_then(|c| c.trim().parse().ok());
    let visto_em = campos
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);

    // Vazio é ausência, e não um nome vazio: é o que uma linha de quatro campos
    // — escrita antes de esta coluna existir — produz, e é a mesma coisa.
    let nome = campos
        .next()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned);

    Some(Conhecido {
        alvo: alvo.to_owned(),
        apelido,
        voice_room,
        visto_em,
        nome,
        // Carregada em `abrir`, que é quem conhece o caminho.
        icone: None,
    })
}

/// Um nome de arquivo estável para um endereço, que não venha do endereço.
///
/// FNV-1a de 64 bits, em hexadecimal. Não é criptografia e não precisa ser: o
/// que se quer é que `../../algo` e `um:endereço/com/barras` virem um nome de
/// arquivo previsível e inofensivo. Escrito à mão pelo motivo de sempre — um
/// crate de digestão para dezesseis dígitos seria caro pelo que entrega.
fn digerir(texto: &str) -> String {
    let mut valor = 0xcbf2_9ce4_8422_2325_u64;
    for byte in texto.as_bytes() {
        valor ^= u64::from(*byte);
        valor = valor.wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("{valor:016x}")
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
    #[test]
    fn o_nome_e_a_imagem_sobrevivem_a_uma_visita_que_nao_os_traz() {
        // `registrar` reescreve a entrada inteira, e é chamado por quem não sabe
        // da aparência — o terminal, por exemplo, que nunca viu o ícone. Sem
        // preservar, cada conexão pelo `connection` apagaria o distintivo que o app
        // tinha anotado, e a lista voltaria a ser uma coluna de endereços.
        let pasta = std::env::temp_dir().join(format!("seele-conhecidos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        let caminho = pasta.join("conhecidos");

        let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");
        lista
            .registrar("server.exemplo:8383", "ayanami", Some(1))
            .expect("registrar");
        lista
            .anotar_aparencia(
                "server.exemplo:8383",
                Some("Casa"),
                Some(&[1, 2, 3]),
            )
            .expect("anotar");

        lista
            .registrar("server.exemplo:8383", "ayanami", Some(2))
            .expect("de novo");

        let de_volta = Conhecidos::abrir(caminho).expect("reabrir");
        let entrada = de_volta.buscar("server.exemplo:8383").expect("está lá");
        assert_eq!(entrada.nome.as_deref(), Some("Casa"));
        assert_eq!(entrada.icone.as_deref(), Some(&[1, 2, 3][..]));
        assert_eq!(entrada.voice_room, Some(2), "a visita nova não valeu");

        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn uma_linha_de_quatro_campos_continua_sendo_lida() {
        // O formato ganhou uma coluna. Uma lista escrita por uma versão
        // anterior tem quatro campos, e recusá-la faria a atualização apagar os
        // atalhos de quem já usava o produto.
        let velha = analisar_linha("server.exemplo:8383\tayanami\t1\t1738000000")
            .expect("uma linha de quatro campos é válida");
        assert_eq!(velha.apelido, "ayanami");
        assert_eq!(velha.voice_room, Some(1));
        assert_eq!(velha.nome, None, "um campo ausente virou nome vazio");
    }

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
        assert_eq!(encontrado.voice_room, Some(1));

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    #[test]
    fn visitar_de_novo_atualiza_em_vez_de_duplicar() {
        // Uma lista de atalhos com o mesmo servidor três vezes é pior que nenhuma.
        let caminho = rascunho("atualiza");
        let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");

        lista
            .registrar("host:8383", "ayanami", Some(1))
            .expect("um");
        lista.registrar("host:8383", "rei", Some(2)).expect("dois");

        assert_eq!(lista.listar().len(), 1);
        let encontrado = lista.buscar("host:8383").expect("achar");
        assert_eq!(encontrado.apelido, "rei");
        assert_eq!(encontrado.voice_room, Some(2));

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
