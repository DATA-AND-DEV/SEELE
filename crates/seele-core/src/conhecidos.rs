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
//! 192.168.0.7:8383 <TAB> marcela <TAB> 1 <TAB> 1738000000
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
    /// Os outros endereços do mesmo servidor, na ordem em que se tenta.
    ///
    /// **Sem isto, voltar a um servidor só tenta o endereço primário do
    /// convite** — que é o da rede local, porque é o que funciona para quem
    /// está na mesma casa e por isso é o primeiro da lista. Quem recebeu o link
    /// pela internet conectava uma vez, pelo caminho de fora, e nunca mais:
    /// a lista de para-onde-voltar tinha guardado o endereço errado dos três.
    ///
    /// Vazio numa entrada escrita antes desta coluna existir, e num servidor
    /// cujo endereço foi digitado à mão — aí não há alternativa que alguém
    /// tenha prometido.
    pub caminhos: Vec<String>,
    /// O bilhete de encontro do convite, como texto. Degrau 4 do ADR 0022.
    ///
    /// Pelo mesmo motivo dos caminhos: sem ele, voltar a um servidor atrás de
    /// NAT perde o degrau que o fez funcionar da primeira vez.
    pub bilhete: Option<String>,
    /// A impressão digital do servidor, como o `seele://` a trouxe.
    ///
    /// **É a única coisa desta linha que não envelhece.** Endereço, caminhos e
    /// bilhete são todos endereço, e endereço atrás de NAT morre quando o
    /// servidor fecha: o roteador dá outro mapeamento na abertura seguinte. A
    /// impressão digital é a mesma para sempre — é a chave do servidor —, e é
    /// dela que sai a marca com que se pergunta ao ponto de encontro «onde esse
    /// servidor está hoje».
    ///
    /// `None` numa entrada escrita antes desta coluna existir, ou numa visita
    /// que veio por endereço digitado e não por link. Aí não há a quem
    /// perguntar, e a lista volta a valer o que valia.
    pub impressao: Option<String>,
    /// O último caminho de subida medido para este servidor, em bits por
    /// segundo.
    ///
    /// `None` numa linha escrita por uma versão anterior, e num servidor onde
    /// ninguém compartilhou tela ainda. Quem o lê é
    /// [`crate::caminho::Sonda::partindo_de`], e o que ele evita são os doze
    /// segundos que a escada gasta para reaprender um cano que já mediu — o
    /// pior momento que a tela tem, e o único que todo mundo vê.
    ///
    /// **Conveniência, e não verdade.** Um número velho não vincula nada: a
    /// sonda continua medindo, e a primeira janela que doer o substitui. É essa
    /// propriedade que deixa ele morar num arquivo que se apaga sem
    /// consequência.
    pub caminho_bps: Option<u32>,
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
        // Os caminhos e o bilhete sobrevivem pela mesma razão que o nome e a
        // imagem: uma visita que não os traz — o terminal, um endereço digitado
        // — não pode apagar o que o convite ensinou.
        let (nome, icone, caminhos, bilhete, impressao) = self
            .entradas
            .iter()
            .find(|e| e.alvo == alvo)
            .map_or((None, None, Vec::new(), None, None), |e| {
                (
                    e.nome.clone(),
                    e.icone.clone(),
                    e.caminhos.clone(),
                    e.bilhete.clone(),
                    e.impressao.clone(),
                )
            });

        // **A medida do caminho sobrevive ao reencontro.** Esta função apaga a
        // entrada e põe outra no lugar, então um campo não repescado aqui é um
        // campo zerado a cada conexão — e uma medida que se perde a cada
        // conexão nunca chega a servir para nada, que é justamente o defeito
        // que ela existe para corrigir.
        let caminho_bps = self
            .entradas
            .iter()
            .find(|e| e.alvo == alvo)
            .and_then(|e| e.caminho_bps);
        self.entradas.retain(|e| e.alvo != alvo);
        self.entradas.push(Conhecido {
            alvo,
            apelido,
            voice_room,
            visto_em: agora,
            nome,
            icone,
            caminhos,
            bilhete,
            impressao,
            caminho_bps,
        });
        self.gravar()
    }

    /// Guarda os outros endereços do mesmo servidor, e o bilhete de encontro.
    ///
    /// Chamado depois de uma conexão que veio de um link: é o link que os
    /// conhece, e é a única vez que este processo os vê.
    ///
    /// # Errors
    ///
    /// O que [`Self::gravar`] devolver.
    pub fn anotar_caminhos(
        &mut self,
        alvo: &str,
        caminhos: &[String],
        bilhete: Option<&str>,
        impressao: Option<&str>,
    ) -> Result<()> {
        let alvo = higienizar(alvo);
        let Some(entrada) = self.entradas.iter_mut().find(|e| e.alvo == alvo) else {
            return Ok(());
        };
        // Só quando há o que guardar: uma reconexão pelo endereço salvo não
        // traz convite, e passar vazio aqui apagaria a escada que o link
        // ensinou — o oposto do que esta função existe para fazer.
        if caminhos.is_empty() && bilhete.is_none() && impressao.is_none() {
            return Ok(());
        }
        if !caminhos.is_empty() || bilhete.is_some() {
            entrada.caminhos = caminhos.iter().map(|c| higienizar(c)).collect();
            entrada.bilhete = bilhete.map(higienizar);
        }
        // **A impressão sobrevive sozinha, e por cima.** Ela não envelhece — é a
        // chave do servidor — e uma visita que não a traz não pode apagá-la, do
        // mesmo jeito que não apaga o nome nem a imagem.
        if let Some(impressao) = impressao {
            entrada.impressao = Some(higienizar(impressao));
        }
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
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    e.alvo,
                    e.apelido,
                    e.voice_room
                        .map_or_else(|| "-".to_owned(), |c| c.to_string()),
                    e.visto_em,
                    // Colunas a mais vão sempre no fim, e nunca no meio: uma
                    // linha de quatro campos escrita por uma versão anterior
                    // continua sendo lida, e cada campo que falta vira ausência.
                    e.nome.as_deref().unwrap_or(""),
                    e.caminhos.join(","),
                    e.bilhete.as_deref().unwrap_or(""),
                    e.impressao.as_deref().unwrap_or(""),
                    e.caminho_bps
                        .map_or_else(String::new, |bps| bps.to_string())
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

    // Os outros endereços, separados por vírgula. Vírgula e não tabulação
    // porque a tabulação é o separador de campo deste arquivo, e um endereço
    // nunca a contém — nem a vírgula, que não é caractere de `host:porta`.
    let caminhos: Vec<String> = campos
        .next()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(|c| c.split(',').map(str::trim).map(str::to_owned).collect())
        .unwrap_or_default();
    let bilhete = campos
        .next()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_owned);
    let impressao = campos
        .next()
        .map(str::trim)
        .filter(|i| !i.is_empty())
        .map(str::to_owned);
    // Um número que não se lê vira ausência, e não zero: zero seria «este cano
    // não carrega nada», que é o contrário de «ninguém mediu ainda».
    let caminho_bps = campos.next().and_then(|c| c.trim().parse().ok());

    Some(Conhecido {
        alvo: alvo.to_owned(),
        apelido,
        voice_room,
        visto_em,
        nome,
        caminhos,
        bilhete,
        impressao,
        caminho_bps,
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
            .registrar("server.exemplo:8383", "marcela", Some(1))
            .expect("registrar");
        lista
            .anotar_aparencia("server.exemplo:8383", Some("Casa"), Some(&[1, 2, 3]))
            .expect("anotar");

        lista
            .registrar("server.exemplo:8383", "marcela", Some(2))
            .expect("de novo");

        let de_volta = Conhecidos::abrir(caminho).expect("reabrir");
        let entrada = de_volta.buscar("server.exemplo:8383").expect("está lá");
        assert_eq!(entrada.nome.as_deref(), Some("Casa"));
        assert_eq!(entrada.icone.as_deref(), Some(&[1, 2, 3][..]));
        assert_eq!(entrada.voice_room, Some(2), "a visita nova não valeu");

        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn a_linha_carrega_o_ultimo_caminho_medido() {
        // **O nono campo, e ele é de conveniência como os outros.** Um arquivo
        // apagado custa doze segundos de imagem ruim uma vez, e nada mais — é
        // por isso que esta medida pode morar aqui, e não junto dos pins.
        let com = analisar_linha(
            "server.exemplo:8383\tmarcela\t1\t1738000000\t\t\t\t\t12480000",
        )
        .expect("uma linha de nove campos é válida");
        assert_eq!(com.caminho_bps, Some(12_480_000));

        // E oito campos — a linha que já está no disco de quem atualizar —
        // continua valendo, com ausência no lugar do número.
        let sem = analisar_linha("server.exemplo:8383\tmarcela\t1\t1738000000")
            .expect("uma linha de quatro campos é válida");
        assert_eq!(sem.caminho_bps, None);
    }

    #[test]
    fn uma_linha_de_quatro_campos_continua_sendo_lida() {
        // O formato ganhou uma coluna. Uma lista escrita por uma versão
        // anterior tem quatro campos, e recusá-la faria a atualização apagar os
        // atalhos de quem já usava o produto.
        let velha = analisar_linha("server.exemplo:8383\tmarcela\t1\t1738000000")
            .expect("uma linha de quatro campos é válida");
        assert_eq!(velha.apelido, "marcela");
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
                .registrar("192.168.0.7:8383", "marcela", Some(1))
                .expect("registrar");
        }

        let lista = Conhecidos::abrir(caminho.clone()).expect("reabrir");
        let encontrado = lista.buscar("192.168.0.7:8383").expect("achar");
        assert_eq!(encontrado.apelido, "marcela");
        assert_eq!(encontrado.voice_room, Some(1));

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    #[test]
    fn visitar_de_novo_atualiza_em_vez_de_duplicar() {
        // Uma lista de atalhos com o mesmo servidor três vezes é pior que nenhuma.
        let caminho = rascunho("atualiza");
        let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");

        lista
            .registrar("host:8383", "marcela", Some(1))
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
                .registrar("host:8383", "mar\tcela\nlima", Some(1))
                .expect("registrar");
        }

        let lista = Conhecidos::abrir(caminho.clone()).expect("reabrir");
        assert_eq!(lista.listar().len(), 1);
        assert_eq!(
            lista.buscar("host:8383").expect("achar").apelido,
            "marcelalima"
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
            "\n\nhost:8383\tmarcela\t1\t100\nlixo sem tabulação\n",
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
        lista.registrar("host:8383", "marcela", None).expect("um");
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
        lista.registrar("host:8383", "marcela", None).expect("um");

        let modo = std::fs::metadata(&caminho)
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(modo & 0o077, 0, "outros conseguem ler a lista");

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }
}

#[cfg(test)]
mod a_escada_do_convite {
    //! Voltar a um servidor tem de tentar os mesmos endereços da primeira vez.
    //!
    //! Um convite traz três caminhos para o mesmo servidor: o da rede local
    //! primeiro, os de fora depois, e o bilhete de encontro para furar o NAT. A
    //! lista de para-onde-voltar guardava **só o primeiro** — e o primeiro é o
    //! da rede local, porque é o que serve a quem está na mesma casa.
    //!
    //! Quem recebeu o link pela internet conectava uma vez, pelo caminho de
    //! fora, e nunca mais: dos três endereços, a lista tinha guardado o único
    //! que não serve a ela.

    use super::*;

    /// Uma pasta só deste teste. `tempfile` não é dependência deste crate, e o
    /// resto do arquivo já usa `temp_dir` com o pid dentro pelo mesmo motivo.
    fn pasta_de(nome: &str) -> std::path::PathBuf {
        let pasta =
            std::env::temp_dir().join(format!("seele-escada-{}-{nome}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).expect("criar a pasta");
        pasta
    }

    fn lista(pasta: &std::path::Path) -> Conhecidos {
        Conhecidos::abrir(pasta.join("conhecidos")).expect("abrir")
    }

    #[test]
    fn os_outros_caminhos_sobrevivem_a_uma_volta_sem_convite() {
        let pasta = pasta_de("volta");
        let mut lista = lista(&pasta);

        // A primeira visita, pelo link: ela conhece a escada inteira.
        lista
            .registrar("192.168.0.7:8383", "aleta", None)
            .expect("registrar");
        lista
            .anotar_caminhos(
                "192.168.0.7:8383",
                &["187.255.97.152:9455".to_owned()],
                Some("encontro.seele.app.br/187.255.97.152:9454"),
                Some("abcdef0123456789cafe"),
            )
            .expect("anotar");

        // A volta, pelo endereço salvo: não há convite nenhum a oferecer.
        lista
            .registrar("192.168.0.7:8383", "aleta", None)
            .expect("registrar");
        lista
            .anotar_caminhos("192.168.0.7:8383", &[], None, None)
            .expect("anotar sem nada");

        let guardado = lista.buscar("192.168.0.7:8383").expect("está na lista");
        assert_eq!(
            guardado.caminhos,
            vec!["187.255.97.152:9455".to_owned()],
            "a segunda visita apagou os endereços de fora, e quem está pela \
             internet não volta mais"
        );
        assert_eq!(
            guardado.bilhete.as_deref(),
            Some("encontro.seele.app.br/187.255.97.152:9454"),
            "o bilhete de encontro sumiu, e com ele o degrau que furava o NAT"
        );
    }

    #[test]
    fn a_escada_sobrevive_a_ida_e_volta_do_arquivo() {
        let pasta = pasta_de("ida-e-volta");
        {
            let mut lista = lista(&pasta);
            lista
                .registrar("casa:8383", "aleta", None)
                .expect("registrar");
            lista
                .anotar_caminhos(
                    "casa:8383",
                    &["fora:9455".to_owned()],
                    Some("ponto/aviso"),
                    None,
                )
                .expect("anotar");
        }
        let relido = lista(&pasta);
        let guardado = relido.buscar("casa:8383").expect("está na lista");
        assert_eq!(guardado.caminhos, vec!["fora:9455".to_owned()]);
        assert_eq!(guardado.bilhete.as_deref(), Some("ponto/aviso"));
    }

    #[test]
    fn uma_linha_de_versao_anterior_continua_sendo_lida() {
        // Cinco campos, que é o que a versão anterior a esta escrevia. As
        // colunas novas viram ausência, e não uma linha descartada — a lista de
        // quem atualiza não pode chegar vazia.
        let caminho = pasta_de("antiga").join("conhecidos");
        std::fs::write(&caminho, "casa:8383\taleta\t-\t1756000000\tCasa\n").expect("escrever");

        let lista = Conhecidos::abrir(caminho).expect("abrir");
        let guardado = lista.buscar("casa:8383").expect("a linha antiga foi lida");
        assert_eq!(guardado.nome.as_deref(), Some("Casa"));
        assert!(guardado.caminhos.is_empty());
        assert_eq!(guardado.bilhete, None);
    }
    #[test]
    fn a_impressao_sobrevive_a_uma_visita_que_nao_a_traz() {
        // **É a única coluna desta linha que não envelhece.** Endereço, caminhos
        // e bilhete são todos endereço, e endereço atrás de NAT morre quando o
        // servidor fecha. A impressão digital é a chave do servidor, e é dela
        // que sai a marca com que se pergunta ao ponto de encontro onde ele
        // está hoje — sem ela, a lista volta a valer o que valia.
        let pasta = pasta_de("impressao");
        let mut lista = lista(&pasta);

        lista
            .registrar("casa:8383", "aleta", None)
            .expect("registrar");
        lista
            .anotar_caminhos("casa:8383", &[], None, Some("abcdef0123456789cafe"))
            .expect("anotar a impressão");

        // Uma volta pelo endereço salvo: sem link, sem convite, sem nada.
        lista
            .registrar("casa:8383", "aleta", None)
            .expect("de novo");
        lista
            .anotar_caminhos("casa:8383", &[], None, None)
            .expect("anotar nada");

        assert_eq!(
            lista.buscar("casa:8383").and_then(|c| c.impressao.clone()),
            Some("abcdef0123456789cafe".to_owned()),
            "a segunda visita apagou a impressão digital, e com ela o único jeito \
             de achar este servidor depois que a porta dele mudar"
        );
    }

    #[test]
    fn uma_linha_de_sete_campos_continua_sendo_lida() {
        // Escrita antes de a coluna da impressão existir. Colunas a mais vão
        // sempre no fim e nunca no meio, e cada campo que falta vira ausência —
        // é a mesma regra que deixou a coluna do bilhete entrar.
        let pasta = pasta_de("sete-campos");
        let caminho = pasta.join("conhecidos");
        std::fs::write(
            &caminho,
            "casa:8383\taleta\t1\t1738000000\tCasa\tfora:9455\tponto/aviso\n",
        )
        .expect("escrever");

        let lista = Conhecidos::abrir(caminho).expect("abrir");
        let guardado = lista.buscar("casa:8383").expect("está na lista");
        assert_eq!(guardado.nome.as_deref(), Some("Casa"));
        assert_eq!(guardado.bilhete.as_deref(), Some("ponto/aviso"));
        assert_eq!(
            guardado.impressao, None,
            "campo que não existe na linha é ausência, e não texto vazio"
        );
    }
}
