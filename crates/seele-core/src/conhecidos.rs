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
//! Acrescentar apelido e última sala de voz ali dentro tornaria esse arquivo maior,
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
//! endereço, apelido, última sala de voz, e quando foi a última visita. Texto porque
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
    /// Última sala de voz em que se entrou.
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
    /// A impressão digital do servidor: a que uma conexão conferida **aceitou**.
    ///
    /// Quase sempre é a que o `seele://` trouxe. Difere quando o link discorda
    /// do pin: a conexão fica de pé com a chave fixada (ADR 0003), e é ela que
    /// se guarda. Guardar a do link fazia a volta pela lista, que confere por
    /// esta impressão, recusar o servidor verdadeiro em todo endereço sem pin.
    /// Quem decide é a casca (`seele_ffi::impressao_a_guardar`).
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
    ///
    /// **Desde a 0.17 é a semente já conferida**, e não o número do disco:
    /// [`crate::caminho::CaminhoLembrado::semente`] contra a data e a rede de
    /// agora, grampeada em [`crate::caminho::TETO_DA_SEMENTE_BPS`]. `None`
    /// também quando a medida é de outra rede, passou de uma semana, ou veio da
    /// 0.16 sem data — o que está guardado é [`Self::caminho_lembrado`].
    pub caminho_bps: Option<u32>,
    /// A medida como está no disco, com a data e a rede em que foi tirada.
    #[serde(skip)]
    pub caminho_lembrado: Option<crate::caminho::CaminhoLembrado>,
}

/// A lista, em disco.
#[derive(Debug, Clone, Default)]
pub struct Conhecidos {
    caminho: PathBuf,
    entradas: Vec<Conhecido>,
    /// A rede em que esta lista foi aberta: contra ela se confere cada
    /// [`Conhecido::caminho_lembrado`], e com ela se anota a medida nova.
    ambiente: String,
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
        Self::abrir_em(caminho, crate::caminho::ambiente_desta_maquina())
    }

    /// [`Self::abrir`] numa rede dita, e não na desta máquina. É por onde os
    /// testes mudam de rede sem mudar de máquina.
    ///
    /// # Errors
    ///
    /// Falha só se o diretório não puder ser criado.
    pub fn abrir_em(caminho: PathBuf, ambiente: String) -> Result<Self> {
        if let Some(pai) = caminho.parent() {
            std::fs::create_dir_all(pai)
                .with_context(|| format!("não consegui criar {}", pai.display()))?;
        }

        let entradas = ler(&caminho, &ambiente).unwrap_or_default();
        Ok(Self {
            caminho,
            entradas,
            ambiente,
        })
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
        self.mudar(|entradas| {
            // O nome e a imagem que já estavam anotados sobrevivem a uma visita que
            // não os traz: `registrar` reescreve a entrada inteira, e apagá-los aqui
            // faria a lista esquecer a aparência a cada conexão do terminal, que não
            // tem como saber dela.
            // Os caminhos e o bilhete sobrevivem pela mesma razão que o nome e a
            // imagem: uma visita que não os traz — o terminal, um endereço digitado
            // — não pode apagar o que o convite ensinou.
            let (nome, icone, caminhos, bilhete, impressao) = entradas
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
            let (caminho_bps, caminho_lembrado) = entradas
                .iter()
                .find(|e| e.alvo == alvo)
                .map_or((None, None), |e| {
                    (e.caminho_bps, e.caminho_lembrado.clone())
                });
            entradas.retain(|e| e.alvo != alvo);
            entradas.push(Conhecido {
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
                caminho_lembrado,
            });
            true
        })
    }

    /// Guarda os outros endereços do mesmo servidor, e o bilhete de encontro.
    ///
    /// Chamado depois de uma conexão que veio de um link: é o link que os
    /// conhece, e é a única vez que este processo os vê.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser gravado.
    pub fn anotar_caminhos(
        &mut self,
        alvo: &str,
        caminhos: &[String],
        bilhete: Option<&str>,
        impressao: Option<&str>,
    ) -> Result<()> {
        let alvo = higienizar(alvo);
        // Só quando há o que guardar: uma reconexão pelo endereço salvo não
        // traz convite, e passar vazio aqui apagaria a escada que o link
        // ensinou — o oposto do que esta função existe para fazer.
        if caminhos.is_empty() && bilhete.is_none() && impressao.is_none() {
            return Ok(());
        }
        self.mudar(|entradas| {
            let Some(entrada) = entradas.iter_mut().find(|e| e.alvo == alvo) else {
                return false;
            };
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
            true
        })
    }

    /// Anota o caminho de subida medido para este servidor.
    ///
    /// Zero **não apaga** o que já estava guardado, e é a mesma regra de
    /// [`Self::anotar_caminhos`]: a sonda só mede enquanto a tela transmite, e
    /// uma sessão em que ninguém compartilhou nada devolveria zero e desfaria,
    /// na segunda visita, o que a primeira aprendeu.
    ///
    /// Anota **quando e em que rede** (a desta lista): é o que a próxima
    /// abertura confere antes de dar a medida como semente.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser gravado.
    pub fn anotar_caminho_medido(&mut self, alvo: &str, bps: u32) -> Result<()> {
        self.anotar_caminho_medido_em(alvo, bps, agora_em_segundos())
    }

    /// [`Self::anotar_caminho_medido`] num instante dito.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser gravado.
    pub fn anotar_caminho_medido_em(&mut self, alvo: &str, bps: u32, agora: i64) -> Result<()> {
        if bps == 0 {
            return Ok(());
        }
        let lembrado = crate::caminho::CaminhoLembrado {
            bps,
            medido_em: agora,
            ambiente: self.ambiente.clone(),
        };
        let semente = lembrado.semente(agora, &self.ambiente);
        self.mudar(|entradas| {
            let Some(entrada) = entradas.iter_mut().find(|e| e.alvo == alvo) else {
                return false;
            };
            entrada.caminho_lembrado = Some(lembrado);
            entrada.caminho_bps = semente;
            true
        })
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
        self.mudar(|entradas| {
            let Some(entrada) = entradas.iter_mut().find(|e| e.alvo == alvo) else {
                return false;
            };
            entrada.nome = nome.map(higienizar);
            entrada.icone = icone.map(<[u8]>::to_vec);
            true
        })
    }

    /// Esquece um servidor.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn esquecer(&mut self, alvo: &str) -> Result<()> {
        self.mudar(|entradas| {
            entradas.retain(|e| e.alvo != alvo);
            true
        })
    }

    /// Muda a lista e grava, quando `mudanca` diz que houve o que gravar.
    ///
    /// **Sobre o arquivo como ele está, e não sobre o que esta lista leu ao
    /// abrir.** O `connect` abre a lista para semear a sonda e para anotar a
    /// visita, a aparência chega depois por outra abertura, e o caminho medido,
    /// ao sair, por mais uma. Duas abertas antes de qualquer uma gravar perdiam
    /// uma das escritas: a segunda regravava a lista velha dela, e apagava o
    /// ícone que a cópia dela não tinha. Agora ler, mudar e gravar acontecem
    /// sob a [`travar`], e a lista é trocada inteira pelo `rename`
    /// ([`trocar_privado`]).
    fn mudar(&mut self, mudanca: impl FnOnce(&mut Vec<Conhecido>) -> bool) -> Result<()> {
        let _trava = travar(&self.caminho)?;
        let mut entradas = ler(&self.caminho, &self.ambiente)
            .with_context(|| format!("não consegui ler {}", self.caminho.display()))?;
        if mudanca(&mut entradas) {
            gravar(&self.caminho, &entradas)?;
        }
        self.entradas = entradas;
        Ok(())
    }
}

/// As entradas do arquivo, com as imagens. Um arquivo que ainda não existe é
/// uma lista vazia, e uma linha ilegível é pulada — inclusive a que não é
/// UTF-8, sem levar as outras junto.
fn ler(caminho: &Path, ambiente: &str) -> std::io::Result<Vec<Conhecido>> {
    let agora = agora_em_segundos();
    let mut entradas: Vec<Conhecido> = match std::fs::read(caminho) {
        Ok(bytes) => String::from_utf8_lossy(&bytes)
            .lines()
            .filter_map(analisar_linha)
            .collect(),
        Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(erro) => return Err(erro),
    };
    // As imagens vêm do disco aqui, e não a cada `listar`: a lista é
    // desenhada na tela de entrada, que a lê mais de uma vez, e reler
    // alguns arquivos por desenho seria pagar disco por uma decoração.
    //
    // Um arquivo que não abre é ausência de imagem, e não erro: a lista de
    // para-onde-voltar não pode deixar de abrir por causa de um enfeite.
    for entrada in &mut entradas {
        entrada.icone = std::fs::read(caminho_do_icone(caminho, &entrada.alvo)).ok();
        // A medida só vira semente se é desta rede e desta semana. A lembrança
        // fica guardada mesmo quando não vale agora: quem volta para casa
        // depois de uma tarde no trabalho reencontra a medida de casa.
        entrada.caminho_bps = entrada
            .caminho_lembrado
            .as_ref()
            .and_then(|lembrado| lembrado.semente(agora, ambiente));
    }
    Ok(entradas)
}

/// Onde a imagem de um servidor é guardada.
///
/// Um arquivo por server, num diretório ao lado da lista. O nome vem do
/// endereço passado por um digestor, e não do endereço: um `alvo` é texto
/// que alguém digitou, e texto que alguém digitou não pode virar caminho de
/// arquivo sem alguém escrever `../` mais cedo ou mais tarde.
fn caminho_do_icone(lista: &Path, alvo: &str) -> PathBuf {
    let mut pasta = lista.to_path_buf();
    let nome = pasta.file_name().map_or_else(
        || "conhecidos".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    pasta.set_file_name(format!("{nome}-icones"));
    pasta.join(format!("{}.png", digerir(alvo)))
}

/// Grava a lista **inteira de uma vez** ([`trocar_privado`]): quem lê no meio
/// — a tela de entrada, enquanto uma sessão anota — vê a de antes ou a de
/// depois, e nunca uma pela metade. Depois, as imagens, uma por arquivo.
fn gravar(caminho: &Path, entradas: &[Conhecido]) -> Result<()> {
    let texto: String = entradas
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
                e.caminho_lembrado
                    .as_ref()
                    .map_or_else(String::new, crate::caminho::CaminhoLembrado::escrever)
            )
        })
        .collect();
    trocar_privado(caminho, texto.as_bytes())?;

    // As imagens, uma por arquivo. Falhar aqui **não** derruba a gravação da
    // lista: um distintivo que não coube em disco é uma tela sem enfeite, e
    // a lista de para-onde-voltar é a coisa que precisa sobreviver.
    for entrada in entradas {
        let do_icone = caminho_do_icone(caminho, &entrada.alvo);
        match entrada.icone.as_deref() {
            Some(bytes) => {
                if let Some(pasta) = do_icone.parent() {
                    let _ = std::fs::create_dir_all(pasta);
                }
                if let Err(erro) = escrever_privado(&do_icone, bytes) {
                    tracing::debug!(%erro, caminho = %do_icone.display(), "não gravei a imagem");
                }
            }
            None => {
                let _ = std::fs::remove_file(&do_icone);
            }
        }
    }
    Ok(())
}

/// Quantos caminhos a lista guarda por servidor, além do endereço da entrada.
///
/// O mesmo teto de um convite: [`seele_proto::uri::LIMITE_DE_ALVOS`] contando
/// o principal, e o principal aqui é o `alvo` da entrada. Cada caminho a mais é
/// uma tentativa a mais antes de a falha aparecer.
const CAMINHOS_POR_SERVIDOR: usize = seele_proto::uri::LIMITE_DE_ALVOS - 1;

/// Os caminhos que a lista guarda, a partir da escada que esta conexão tentou.
///
/// É o que [`Conhecidos::anotar_caminhos`] recebe depois de uma conexão. Duas
/// regras, e as duas existem pelo mesmo defeito: com um NAT que troca de porta a
/// cada abertura do anfitrião, a lista ganhava um endereço morto por
/// reabertura, todos entravam na corrida, e a falha ficava mais lenta a cada
/// semana.
///
/// - **O endereço que veio do quarto não entra.** Ele é o de hoje, e na próxima
///   conexão é perguntado de novo: guardá-lo só serviria para ele virar o morto
///   da reabertura seguinte. `do_quarto` é o endereço que a consulta pôs na
///   frente da escada, e só quando foi ela que o pôs: um que já estava na
///   escada veio do link ou da lista, e continua.
/// - **Cabem [`CAMINHOS_POR_SERVIDOR`]**, os primeiros da escada, na ordem em
///   que foram tentados. Uma lista gravada antes desta regra encolhe na
///   primeira conexão que der certo.
#[must_use]
pub fn caminhos_a_guardar(tentados: &[String], do_quarto: Option<&str>) -> Vec<String> {
    tentados
        .iter()
        .filter(|caminho| Some(caminho.as_str()) != do_quarto)
        .take(CAMINHOS_POR_SERVIDOR)
        .cloned()
        .collect()
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
    // não carrega nada», que é o contrário de «ninguém mediu ainda». E o
    // número solto que a 0.16 gravava, sem data nem rede, também: não há como
    // saber se ele descreve o caminho de hoje.
    let caminho_lembrado = campos.next().and_then(crate::caminho::CaminhoLembrado::ler);

    Some(Conhecido {
        alvo: alvo.to_owned(),
        apelido,
        voice_room,
        visto_em,
        nome,
        caminhos,
        bilhete,
        impressao,
        // Conferida em `ler`, que é quem sabe a hora e a rede.
        caminho_bps: None,
        caminho_lembrado,
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
///
/// A agenda da sala pessoal grava pelo mesmo caminho ([`crate::agenda`]).
#[cfg(unix)]
pub(crate) fn escrever_privado(caminho: &Path, bytes: &[u8]) -> std::io::Result<()> {
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
pub(crate) fn escrever_privado(caminho: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(caminho, bytes)
}

/// O arquivo ao lado deste, com o sufixo: `conhecidos.trava`, `agenda.nova`.
pub(crate) fn ao_lado(caminho: &Path, sufixo: &str) -> PathBuf {
    let mut nome = caminho.file_name().unwrap_or_default().to_os_string();
    nome.push(".");
    nome.push(sufixo);
    caminho.with_file_name(nome)
}

/// **A trava das gravações** de um arquivo regravado inteiro — esta lista e a
/// [`crate::agenda`] —, num arquivo ao lado que nunca é trocado: o arquivo é
/// trocado a cada gravação ([`trocar_privado`]), e uma trava nele ficaria
/// presa no arquivo velho.
///
/// `File::lock` é o `flock` no Unix e o `LockFileEx` no Windows: vale entre
/// dois processos e entre duas aberturas no mesmo processo, e sai quando o
/// arquivo fecha. Num sistema sem trava de arquivo a gravação segue sem ela, e
/// o `seele.log` diz.
///
/// # Errors
///
/// Falha se o caminho não nomear um arquivo, ou se a trava não abrir.
pub(crate) fn travar(caminho: &Path) -> Result<Option<std::fs::File>> {
    if caminho.file_name().is_none() {
        anyhow::bail!("{} não é um arquivo para travar", caminho.display());
    }
    let trava = ao_lado(caminho, "trava");
    let mut opcoes = std::fs::OpenOptions::new();
    opcoes.create(true).write(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opcoes, 0o600);
    let arquivo = opcoes
        .open(&trava)
        .with_context(|| format!("não consegui abrir {}", trava.display()))?;
    match arquivo.lock() {
        Ok(()) => Ok(Some(arquivo)),
        Err(erro) if erro.kind() == std::io::ErrorKind::Unsupported => {
            tracing::warn!(
                %erro,
                arquivo = %caminho.display(),
                "este sistema não trava arquivos: duas gravações ao mesmo tempo podem perder \
                 uma delas"
            );
            Ok(None)
        }
        Err(erro) => Err(erro).with_context(|| format!("não consegui travar {}", trava.display())),
    }
}

/// Grava um arquivo **inteiro de uma vez**: num arquivo ao lado, só para o
/// dono, trocado pelo `rename`. Quem lê no meio vê o de antes ou o de depois,
/// e nunca um pela metade.
///
/// # Errors
///
/// Falha se o arquivo ao lado não puder ser gravado, ou a troca não acontecer.
pub(crate) fn trocar_privado(caminho: &Path, bytes: &[u8]) -> Result<()> {
    let nova = ao_lado(caminho, "nova");
    escrever_privado(&nova, bytes)
        .with_context(|| format!("não consegui gravar {}", nova.display()))?;
    std::fs::rename(&nova, caminho)
        .with_context(|| format!("não consegui trocar {} pela nova", caminho.display()))
}

pub(crate) fn agora_em_segundos() -> i64 {
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
            "server.exemplo:8383\tmarcela\t1\t1738000000\t\t\t\t\t12480000@1738000000@enlace-0.17|10.0.0.5|-",
        )
        .expect("uma linha de nove campos é válida");
        assert_eq!(
            com.caminho_lembrado,
            Some(crate::caminho::CaminhoLembrado {
                bps: 12_480_000,
                medido_em: 1_738_000_000,
                ambiente: "enlace-0.17|10.0.0.5|-".to_owned(),
            })
        );

        // O número solto da 0.16, sem data nem rede, não é lembrança.
        let da_016 =
            analisar_linha("server.exemplo:8383\tmarcela\t1\t1738000000\t\t\t\t\t12480000")
                .expect("a linha da 0.16 continua válida");
        assert_eq!(da_016.caminho_lembrado, None);
        assert_eq!(da_016.caminho_bps, None);

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
        // O relógio tem segundos de resolução, então o primeiro registro é
        // envelhecido à mão para o teste não depender de esperar um segundo.
        // No arquivo, e não na memória: cada gravação relê a lista do disco.
        let visto_em = lista.entradas[0].visto_em;
        let texto = std::fs::read_to_string(&caminho).expect("ler");
        std::fs::write(
            &caminho,
            texto.replace(
                &format!("\t{visto_em}\t"),
                &format!("\t{}\t", visto_em - 100),
            ),
        )
        .expect("envelhecer");
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

    /// **Duas listas abertas antes de qualquer uma gravar.** O `connect` abre a
    /// lista para semear a sonda e para anotar a visita, a aparência chega
    /// depois por outra abertura, e o caminho medido, ao sair, por mais uma. A
    /// segunda a gravar regravava a lista que leu ao abrir: apagava o que a
    /// primeira tinha escrito, e o ícone que a cópia dela não tinha.
    #[test]
    fn duas_listas_abertas_antes_de_gravar_nao_perdem_nenhuma_escrita() {
        let caminho = rascunho("duas-abertas");
        Conhecidos::abrir(caminho.clone())
            .expect("abrir")
            .registrar("casa.exemplo:8383", "marcela", Some(1))
            .expect("a visita");
        let mut da_aparencia = Conhecidos::abrir(caminho.clone()).expect("abrir");
        let mut da_saida = Conhecidos::abrir(caminho.clone()).expect("abrir");

        da_aparencia
            .anotar_aparencia("casa.exemplo:8383", Some("Casa"), Some(&[1, 2, 3]))
            .expect("a aparência");
        da_aparencia
            .registrar("outro.exemplo:8383", "marcela", None)
            .expect("outra visita");
        // O caminho medido do mesmo servidor, gravado por quem abriu antes de
        // a aparência chegar, e um servidor novo, acrescentado quase junto.
        da_saida
            .anotar_caminho_medido("casa.exemplo:8383", 12_480_000)
            .expect("o caminho medido");
        da_saida
            .registrar("mais.exemplo:8383", "marcela", None)
            .expect("mais uma visita");

        let de_volta = Conhecidos::abrir(caminho.clone()).expect("reabrir");
        let casa = de_volta
            .buscar("casa.exemplo:8383")
            .expect("o servidor visitado sumiu da lista");
        assert_eq!(
            (
                casa.nome.as_deref(),
                casa.icone.as_deref(),
                casa.caminho_bps
            ),
            (
                Some("Casa"),
                Some(&[1, 2, 3][..]),
                Some(crate::caminho::TETO_DA_SEMENTE_BPS)
            ),
            "a aparência e o caminho medido do mesmo servidor não ficaram os dois"
        );
        assert_eq!(
            casa.caminho_lembrado.as_ref().map(|l| l.bps),
            Some(12_480_000),
            "o disco guarda a medida inteira; só a semente é grampeada"
        );
        assert!(
            de_volta.buscar("outro.exemplo:8383").is_some(),
            "a visita que a aparência acrescentou se perdeu quando o caminho foi gravado"
        );
        assert!(de_volta.buscar("mais.exemplo:8383").is_some());
        assert_eq!(
            da_saida.listar().len(),
            3,
            "a lista que gravou o caminho não ficou com o que está no arquivo"
        );

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    /// **Uma medida de outra rede, de outra semana ou da 0.16 não é semente.**
    ///
    /// Decisão da 0.17: a estimativa lembrada não pode dar confiança ilimitada
    /// ao caminho de hoje. A lembrança é conferida pela rede (o ambiente) e pela
    /// idade, e grampeada em [`crate::caminho::TETO_DA_SEMENTE_BPS`].
    #[test]
    fn a_medida_lembrada_so_vale_na_mesma_rede_na_mesma_semana_e_com_teto() {
        use crate::caminho::{IDADE_MAXIMA_DA_LEMBRANCA_S, TETO_DA_SEMENTE_BPS};
        let caminho = rascunho("lembranca");
        let casa = "enlace-0.17|192.168.0.10|-".to_owned();
        let trabalho = "enlace-0.17|10.20.0.33|-".to_owned();
        let agora = agora_em_segundos();

        let mut lista = Conhecidos::abrir_em(caminho.clone(), casa.clone()).expect("abrir");
        lista
            .registrar("server.exemplo:8383", "marcela", None)
            .expect("registrar");
        lista
            .anotar_caminho_medido_em("server.exemplo:8383", 4_000_000, agora)
            .expect("anotar");

        let semente = |ambiente: &str| {
            Conhecidos::abrir_em(caminho.clone(), ambiente.to_owned())
                .expect("reabrir")
                .buscar("server.exemplo:8383")
                .and_then(|c| c.caminho_bps)
        };
        assert_eq!(semente(&casa), Some(4_000_000), "na mesma rede, vale");
        assert_eq!(
            semente(&trabalho),
            None,
            "a medida de casa semeou o trabalho"
        );
        assert_eq!(
            semente("quic-0.16|192.168.0.10|-"),
            None,
            "uma medida da linha anterior semeou o enlace"
        );
        // E a lembrança não se perde por ter sido aberta noutra rede.
        assert_eq!(
            semente(&casa),
            Some(4_000_000),
            "a volta para casa esqueceu"
        );

        // Velha demais.
        lista
            .anotar_caminho_medido_em(
                "server.exemplo:8383",
                4_000_000,
                agora - IDADE_MAXIMA_DA_LEMBRANCA_S - 1,
            )
            .expect("anotar velha");
        assert_eq!(
            semente(&casa),
            None,
            "uma medida de mais de uma semana semeou"
        );

        // Do futuro: relógio errado.
        lista
            .anotar_caminho_medido_em("server.exemplo:8383", 4_000_000, agora + 86_400)
            .expect("anotar do futuro");
        assert_eq!(semente(&casa), None, "uma medida com data no futuro semeou");

        // E o teto: uma medida grande vira, no máximo, a semente do teto.
        lista
            .anotar_caminho_medido_em("server.exemplo:8383", 90_000_000, agora)
            .expect("anotar grande");
        assert_eq!(semente(&casa), Some(TETO_DA_SEMENTE_BPS));

        // A linha da 0.16, sem data, no lugar da coluna: sem semente.
        let texto = std::fs::read_to_string(&caminho).expect("ler");
        let antiga: String = texto
            .lines()
            .map(|linha| {
                let mut campos: Vec<&str> = linha.split('\t').collect();
                if let Some(ultimo) = campos.last_mut() {
                    *ultimo = "12480000";
                }
                format!("{}\n", campos.join("\t"))
            })
            .collect();
        std::fs::write(&caminho, antiga).expect("escrever a da 0.16");
        assert_eq!(semente(&casa), None, "o número solto da 0.16 semeou a 0.17");

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    /// **Muitas escritas ao mesmo tempo, cada uma pela sua lista.** Várias
    /// gravações por linha: a trava posta na própria lista, que o `rename`
    /// troca, só deixa duas gravações entrarem juntas quando três se cruzam.
    #[test]
    fn escritas_ao_mesmo_tempo_nao_perdem_nenhum_servidor() {
        const LINHAS: usize = 16;
        const POR_LINHA: usize = 8;
        let caminho = rascunho("ao-mesmo-tempo");
        let largada = std::sync::Arc::new(std::sync::Barrier::new(LINHAS));
        let linhas: Vec<_> = (0..LINHAS)
            .map(|linha| {
                let caminho = caminho.clone();
                let largada = std::sync::Arc::clone(&largada);
                std::thread::spawn(move || {
                    let mut lista = Conhecidos::abrir(caminho).expect("abrir");
                    largada.wait();
                    for n in linha * POR_LINHA..(linha + 1) * POR_LINHA {
                        lista
                            .registrar(&format!("servidor-{n}.exemplo:8383"), "marcela", None)
                            .expect("registrar");
                    }
                })
            })
            .collect();
        for linha in linhas {
            linha.join().expect("uma escrita caiu");
        }

        let de_volta = Conhecidos::abrir(caminho.clone()).expect("reabrir");
        let faltam: Vec<usize> = (0..LINHAS * POR_LINHA)
            .filter(|n| {
                de_volta
                    .buscar(&format!("servidor-{n}.exemplo:8383"))
                    .is_none()
            })
            .collect();
        assert!(
            faltam.is_empty(),
            "escritas ao mesmo tempo perderam {} de {} servidores: {faltam:?}",
            faltam.len(),
            LINHAS * POR_LINHA
        );

        let _ = std::fs::remove_dir_all(caminho.parent().expect("pai"));
    }

    /// **Quem lê no meio de uma gravação vê a lista inteira**, a de antes ou a
    /// de depois. A tela de entrada lê a lista enquanto uma sessão anota nela,
    /// e uma lista lida pela metade mostraria servidores a menos.
    #[test]
    fn quem_le_no_meio_de_uma_gravacao_ve_a_lista_inteira() {
        const ANTES: u32 = 120;
        let caminho = rascunho("no-meio");
        let mut lista = Conhecidos::abrir(caminho.clone()).expect("abrir");
        for n in 0..ANTES {
            lista
                .registrar(&format!("servidor-{n}.exemplo:8383"), "marcela", None)
                .expect("registrar");
        }

        let parar = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let leitora = {
            let caminho = caminho.clone();
            let parar = std::sync::Arc::clone(&parar);
            std::thread::spawn(move || {
                let mut menor = usize::MAX;
                let mut leituras = 0_u32;
                while !parar.load(std::sync::atomic::Ordering::Relaxed) {
                    let vistos = Conhecidos::abrir(caminho.clone())
                        .expect("abrir")
                        .listar()
                        .len();
                    menor = menor.min(vistos);
                    leituras += 1;
                }
                (menor, leituras)
            })
        };
        for n in 0..ANTES {
            // Um caminho medido a cada gravação: a lista é regravada inteira.
            lista
                .anotar_caminho_medido(&format!("servidor-{n}.exemplo:8383"), 1_000_000 + n)
                .expect("o caminho medido");
        }
        parar.store(true, std::sync::atomic::Ordering::Relaxed);
        let (menor, leituras) = leitora.join().expect("a leitora caiu");

        assert!(leituras > 0, "a leitora não leu nenhuma vez");
        assert_eq!(
            menor, ANTES as usize,
            "uma leitura no meio da gravação viu {menor} de {ANTES} servidores, em {leituras} \
             leituras"
        );

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

    #[test]
    fn o_endereco_do_quarto_nao_vai_para_a_lista_e_a_lista_tem_teto() {
        // O defeito: o endereço que o quarto dá entra na frente da escada, e a
        // escada inteira ia para a lista. Com um NAT que troca de porta a cada
        // abertura do anfitrião, cada volta gravava mais um, e na seguinte ele
        // já era um morto: `[X_k … X_1, originais]`, todos na corrida, cada um
        // com a sua defasagem.
        let originais: Vec<String> = [
            "192.168.0.7:8383",
            "[2804:388::7]:8383",
            "187.255.97.152:9455",
        ]
        .map(str::to_owned)
        .to_vec();
        let mut guardados = originais.clone();
        for reabertura in 1..=5_u16 {
            // O anfitrião reabriu, e o quarto diz a porta de hoje, que o app
            // põe na frente da escada.
            let de_hoje = format!("187.255.97.152:{}", 40_000 + reabertura);
            let mut tentados = guardados.clone();
            tentados.insert(0, de_hoje.clone());
            guardados = caminhos_a_guardar(&tentados, Some(&de_hoje));
        }
        assert_eq!(
            guardados, originais,
            "o endereço que o quarto deu foi gravado na lista: a cada reabertura do anfitrião \
             ela ganha um endereço morto, todos entram na corrida, e a falha fica mais lenta a \
             cada semana"
        );

        // O teto: uma lista gravada antes deste conserto, ou por qualquer outro
        // caminho, não cresce sem fim.
        let muitos: Vec<String> = (0..10_u16)
            .map(|porta| format!("187.255.97.152:{}", 41_000 + porta))
            .collect();
        let guardado = caminhos_a_guardar(&muitos, None);
        let cabem = seele_proto::uri::LIMITE_DE_ALVOS - 1;
        assert_eq!(
            guardado,
            muitos.iter().take(cabem).cloned().collect::<Vec<_>>(),
            "a lista gravou {} caminhos para um servidor, e não os {cabem} primeiros da \
             escada: sem teto, cada caminho a mais é uma tentativa a mais antes de a falha \
             aparecer",
            guardado.len()
        );
    }
}
