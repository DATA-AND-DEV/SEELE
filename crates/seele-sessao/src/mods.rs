//! **Um pacote de MOD, pelos bytes** (Tarefa 2.6 da F4): o que o desktop lê de
//! uma pasta e o navegador lê do armazenamento da página, pelas mesmas regras.
//!
//! O que um manifesto é, e o que o recusa, mora no `seele_proto::mods`. O que
//! está aqui é o que o `seele-core` e o `seele-ffi` faziam com ele sem precisar
//! de disco: o nome da recusa, o [`ModInstalado`] que a janela desenha, a
//! conferência do conteúdo contra o lugar em que ele foi guardado, o tipo de uma
//! mídia pelos bytes e o `data:` dela. O `seele-core` reexporta tudo, e o
//! desktop não mudou; o disco (ler a pasta, juntar os arquivos) ficou lá.

pub use seele_proto::mods::{
    capacidades_da_api, como_ele_se_chama, content_hash, hex, ComoEleSeChama, Manifest, Refused,
};

/// Os arquivos de um pacote: o caminho de dentro, com `/`, e os bytes.
pub type Arquivos = Vec<(String, Vec<u8>)>;

/// Um pacote guardado: o lugar (o hash pelo qual foi guardado) e os arquivos.
pub type Guardado = (String, Arquivos);

/// The name of a refusal, from a closed list.
///
/// Exists here and not in the shell for the reason above: the shell cannot name
/// `Refused`'s variants. ADR 0012 keeps the wording in the shell — this is an
/// identifier, not a sentence, and `ui/frases.js` turns it into one.
#[must_use]
pub fn refusal_name(why: &Refused) -> &'static str {
    match why {
        Refused::Malformed { .. } => "malformed",
        Refused::SchemaTooNew { .. } => "schema-too-new",
        Refused::ApiTooNew { .. } => "api-too-new",
        Refused::ApiTooOld { .. } => "api-too-old",
        Refused::MalformedId => "malformed-id",
        Refused::Empty => "empty",
        Refused::TemaSemMetadeDeServidor => "tema-sem-metade-de-servidor",
        Refused::ArquivoInvalido { .. } => "arquivo-invalido",
        Refused::ArquivosDemais { .. } => "arquivos-demais",
        // Os de cargos (E6, agente F): a frase é do `ui/frases.js`.
        Refused::PermissoesPedemApiNova { .. } => "permissoes-pedem-api-nova",
        Refused::PermissoesInvalidas(_) => "permissoes-invalidas",
        // Os tetos do `reach` no anúncio da linha 2, conferidos na instalação.
        Refused::AlcancesDemais { .. } => "alcances-demais",
        Refused::AlcanceGrandeDemais { .. } => "alcance-grande-demais",
    }
}

/// Um MOD guardado, como a janela o desenha.
///
/// Um MOD recusado chega com `refused` preenchido em vez de ficar de fora da
/// lista: quem largou o diretório ali tem direito ao motivo, e o defeito que o
/// `CLAUDE.md` deste repositório nomeia como o mais caro é «o produto sabe e não
/// conta».
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModInstalado {
    /// `autor/nome`.
    pub id: String,
    /// A versão que o autor declara, ou vazio quando o manifesto não foi lido.
    pub version: String,
    /// **A API que este pacote declara**, e não a que este build oferece.
    ///
    /// Desde a API 4 o produto executa um conjunto de versões, e não uma só
    /// (ver `seele_proto::mods::APIS_ACEITAS`). O que um pacote pode chamar
    /// passa a depender do que **ele** declarou, e é este campo que o prelúdio
    /// lê para montá-lo.
    ///
    /// Zero quando o manifesto não foi lido — e zero não dá capacidade nenhuma.
    pub api: u32,
    /// O hash do conteúdo em hexadecimal, ou vazio. É o que uma pessoa compara
    /// a olho com o que o indexador publica.
    pub hash: String,
    /// O caminho do script que a janela carrega, se há metade de cliente.
    pub client: Option<String>,
    /// Os arquivos que este MOD traz para mostrar ou tocar.
    ///
    /// **Declarados no manifesto**, e é por esta lista que a leitura passa: um
    /// arquivo no pacote que ninguém nomeou não é servido. Vai para a tela de
    /// aceite pela mesma razão que [`Self::reach`] vai — quem instala lê o que
    /// o pacote traz antes de dizer sim.
    pub arquivos: Vec<String>,
    /// O repositório público que o manifesto declara. ADR 0045 o torna
    /// condição de publicação, e é o que uma pessoa abre para ler o que vai
    /// rodar na máquina dela.
    pub repo: String,
    /// O que este MOD declara alcançar.
    ///
    /// Vai para a tela **antes** de qualquer byte ser baixado: ADR 0045 exige
    /// que quem entra leia o alcance declarado antes de aceitar.
    pub reach: Vec<String>,
    /// Se este MOD tem metade de servidor.
    ///
    /// **A linha que a tela de aceite não pode omitir.** Um MOD com metade de
    /// servidor roda na máquina de quem hospeda e alcança o bloco `world` do
    /// `api/v1.json` — rede de saída, relógio e log.
    pub server: bool,
    /// **A porta do tema** (rascunho da API 6, §9): o manifesto declara
    /// `"tema": true`, e a metade de servidor pode pintar a aparência do
    /// servidor inteiro.
    pub tema: bool,
    /// O nome da recusa, de uma lista fechada, quando houve uma.
    pub refused: Option<String>,
}

impl ModInstalado {
    /// Um pacote que leu limpo, sob o hash dele em hexadecimal.
    #[must_use]
    pub fn lido(manifest: Manifest, hash: String) -> Self {
        Self {
            id: manifest.id,
            version: manifest.version,
            api: manifest.api,
            hash,
            client: manifest.client,
            arquivos: manifest.arquivos,
            repo: manifest.repo,
            reach: manifest.reach,
            server: manifest.server.is_some(),
            tema: manifest.tema,
            refused: None,
        }
    }

    /// **O que ele diz ser, quando dá para ler** — U10.
    ///
    /// A recusa continua total para executar: nada aqui faz um pacote
    /// incompatível rodar, e `refused` continua preenchido. O que ela deixa
    /// de ser é total para **descrever**: quem vai decidir entre apagar e
    /// atualizar precisa do nome e da versão, e o nome do lugar é um hash.
    #[must_use]
    pub fn recusado(id: String, why: &Refused, diz_ser: ComoEleSeChama) -> Self {
        Self {
            id: if diz_ser.id.is_empty() {
                id
            } else {
                diz_ser.id
            },
            version: diz_ser.version,
            // A API que ele **pede**, e não a que este build oferece. É o
            // número que diz se atualizar o aplicativo resolve, ou se é o
            // pacote que precisa de uma versão nova.
            api: diz_ser.api,
            hash: String::new(),
            client: None,
            arquivos: Vec::new(),
            repo: String::new(),
            reach: Vec::new(),
            server: false,
            tema: false,
            refused: Some(refusal_name(why).to_owned()),
        }
    }
}

/// O hash do conjunto de arquivos de um MOD, em hexadecimal minúsculo.
///
/// O **mesmo** número que o servidor anuncia, que a tela de aceite mostra e que
/// o catálogo do indexador publica — e é essa unicidade que faz a conferência
/// valer.
///
/// Toma `&mut` porque a ordenação acontece em quem digere: uma lista de
/// arquivos em ordem diferente tem de dar o mesmo hash em toda máquina.
#[must_use]
pub fn hash_do_conjunto(arquivos: &mut [(String, Vec<u8>)]) -> String {
    hex(&content_hash(arquivos))
}

/// O manifesto que os arquivos dizem ter, sem validar nada: o `mod.json` de
/// cima, se há um e se ele é texto.
fn texto_do_manifesto(arquivos: &[(String, Vec<u8>)]) -> Option<&str> {
    arquivos
        .iter()
        .find(|(caminho, _)| caminho == "mod.json")
        .and_then(|(_, bytes)| std::str::from_utf8(bytes).ok())
}

/// **Um pacote, dos arquivos dele**: os caminhos relativos com `/`, e os
/// bytes. O manifesto validado e o hash do conteúdo, como o `read_one` do
/// `seele-core` os lê de uma pasta.
///
/// # Errors
///
/// [`Refused::Malformed`] na primeira posição quando não há `mod.json` (ou ele
/// não é texto), e a recusa do `read_manifest` quando ele não vale.
pub fn ler_pacote(arquivos: &mut [(String, Vec<u8>)]) -> Result<(Manifest, [u8; 32]), Refused> {
    let texto = texto_do_manifesto(arquivos).ok_or(Refused::Malformed { line: 1, column: 1 })?;
    let manifest = seele_proto::mods::read_manifest(texto)?;
    Ok((manifest, content_hash(arquivos)))
}

/// **O pacote pedido por um hash**: o que leu, conferido contra o hash pelo
/// qual foi pedido. A decisão do `ler_por_hash` da FFI, que a chama com o que
/// leu do disco, e do navegador, com o que leu do armazenamento da página.
///
/// # Errors
///
/// `conteudo-nao-bate` quando os bytes não são os que o hash promete — servir
/// assim mesmo seria entregar à janela bytes que ninguém revisou sob o hash de
/// bytes que alguém revisou —, e o nome da recusa do manifesto.
pub fn pelo_hash(
    lido: Result<(Manifest, [u8; 32]), Refused>,
    hash: &str,
) -> Result<ModInstalado, String> {
    match lido {
        Ok((manifest, conteudo)) if hex(&conteudo) == hash => {
            Ok(ModInstalado::lido(manifest, hash.to_owned()))
        }
        Ok(_) => Err("conteudo-nao-bate".to_owned()),
        Err(motivo) => Err(refusal_name(&motivo).to_owned()),
    }
}

/// **Um pacote guardado, como a lista o mostra**: conferido contra o lugar em
/// que está guardado (o hash), e recusado e nomeado quando não bate. A regra do
/// `listar_por_conteudo` do `seele-core`, para quem guarda pelos bytes e não
/// por pasta.
#[must_use]
pub fn guardado_em(mut arquivos: Arquivos, lugar: &str) -> ModInstalado {
    let diz_ser = texto_do_manifesto(&arquivos)
        .map(como_ele_se_chama)
        .unwrap_or_default();
    match ler_pacote(&mut arquivos) {
        Ok((manifest, conteudo)) if hex(&conteudo) == lugar => {
            ModInstalado::lido(manifest, lugar.to_owned())
        }
        // O conteúdo não é o que o lugar promete, e nada assim pode ser servido
        // a ninguém como se fosse revisado.
        Ok((manifest, _)) => ModInstalado::recusado(manifest.id, &Refused::MalformedId, diz_ser),
        Err(why) => ModInstalado::recusado(lugar.to_owned(), &why, diz_ser),
    }
}

/// **Os pacotes guardados, como a lista os mostra**: cada um pelo
/// [`guardado_em`], na ordem do `listar_por_conteudo` do `seele-core` (o `id`
/// que o manifesto diz, ou o lugar quando nem ele abriu).
#[must_use]
pub fn listar_guardados(guardados: Vec<Guardado>) -> Vec<ModInstalado> {
    let mut lista: Vec<(String, ModInstalado)> = guardados
        .into_iter()
        .map(|(lugar, mut arquivos)| {
            let ordem = match ler_pacote(&mut arquivos) {
                Ok((manifest, _)) => manifest.id,
                Err(_) => lugar.clone(),
            };
            (ordem, guardado_em(arquivos, &lugar))
        })
        .collect();
    lista.sort_by(|esquerda, direita| esquerda.0.cmp(&direita.0));
    lista.into_iter().map(|(_, instalado)| instalado).collect()
}

/// **O arquivo que um MOD serve**: o caminho de dentro do pacote,
/// reconstruído por componentes, quando `url_path` (`/autor/nome/dentro…`) é
/// deste pacote e nomeia o script do cliente ou um dos `arquivos` declarados.
///
/// `None` para todo o resto, e de propósito a mesma resposta para «não existe»,
/// «não foi declarado» e «tentou subir»: um esquema que os distinguisse
/// responderia perguntas sobre o disco de quem o roda. O `serve` do desktop
/// lê do disco o que esta regra deixa; o navegador, do armazenamento da
/// página.
#[must_use]
pub fn caminho_declarado(pacote: &ModInstalado, url_path: &str) -> Option<std::path::PathBuf> {
    let mut partes = url_path.trim_start_matches('/').split('/');
    let autor = partes.next()?;
    let nome = partes.next()?;
    let dentro: Vec<&str> = partes.collect();
    if dentro.is_empty() || autor.is_empty() || nome.is_empty() {
        return None;
    }
    let relativo = seele_proto::mods::inner_path(&dentro)?;
    if pacote.id != format!("{autor}/{nome}") {
        return None;
    }
    // Só o que o manifesto declara: o script do cliente **e** os arquivos que
    // ele lista em `arquivos`.
    pacote
        .client
        .iter()
        .chain(pacote.arquivos.iter())
        .any(|declarado| std::path::Path::new(declarado) == relativo.as_path())
        .then_some(relativo)
}

/// Um arquivo de mídia de um MOD, lido e pronto para uma janela montar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiaDeMod {
    /// O `data:` inteiro, com o tipo que os bytes provaram ser.
    pub uri: String,
    /// `som` ou `imagem`, de uma lista fechada.
    pub papel: &'static str,
    /// Quantos bytes o arquivo tem.
    pub bytes: usize,
}

/// **O teto por arquivo de mídia de um MOD.**
pub use seele_proto::midia_de_mod::TETO_DE_ARQUIVO as TETO_DE_MIDIA;

/// Um formato que o produto reconhece e não aceita (o HEIC), para a recusa
/// dizer o nome dele — ver [`seele_proto::imagem::reconhecido_e_nao_aceito`].
pub use seele_proto::imagem::reconhecido_e_nao_aceito;

/// O que estes bytes são, **sem montar um `data:` deles**.
///
/// [`ler_midia`] serve para o que vai virar um elemento na hora: ela devolve o
/// `data:` inteiro, e para isso codifica tudo em base64. Um arquivo que uma
/// pessoa acabou de escolher pode ter dez megabytes e vai sair em pedaços —
/// codificá-lo inteiro para descobrir que ele é um PNG seria pagar treze
/// megabytes de texto por uma pergunta de oito bytes.
///
/// O teto não é conferido aqui: quem escolhe o arquivo tem o próprio, e ele é
/// maior que o da mídia de pacote.
#[must_use]
pub fn ler_tipo(bytes: &[u8]) -> Option<TipoLido> {
    let tipo = seele_proto::midia_de_mod::sniff(bytes)?;
    Some(TipoLido {
        media_type: tipo.media_type(),
        papel: tipo.papel(),
    })
}

/// O que os bytes provaram ser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TipoLido {
    /// `image/png`, `audio/wav`, e assim por diante.
    pub media_type: &'static str,
    /// `som` ou `imagem`.
    pub papel: &'static str,
}

/// Lê um arquivo de mídia de MOD, ou diz que não.
///
/// **Aqui, e não na casca.** A composição do `data:` é a mesma decisão do ADR
/// 0027 para anexos: uma casca que junta um tipo a bytes é uma casca que pode
/// juntar a alegação de quem mandou a eles. O manifesto de um MOD é
/// exatamente essa alegação, e por isso o tipo sai dos bytes e o `data:` sai
/// daqui inteiro.
///
/// Nada para o que passa do teto e para o que não é um formato conhecido: são
/// recusas diferentes para quem está escrevendo o MOD, e quem as separa é a
/// casca, que já conhece o tamanho.
#[must_use]
pub fn ler_midia(bytes: &[u8]) -> Option<MidiaDeMod> {
    if bytes.len() > TETO_DE_MIDIA {
        return None;
    }
    let tipo = seele_proto::midia_de_mod::sniff(bytes)?;
    // O mesmo codificador do `data:` de um anexo. Um segundo aqui seria um
    // segundo alfabeto para o mesmo RFC.
    let mut uri = String::with_capacity(bytes.len().div_ceil(3) * 4 + 32);
    uri.push_str("data:");
    uri.push_str(tipo.media_type());
    uri.push_str(";base64,");
    crate::preview::encode_base64_publico(bytes, &mut uri);
    Some(MidiaDeMod {
        uri,
        papel: tipo.papel(),
        bytes: bytes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pacote(manifesto: &str) -> Vec<(String, Vec<u8>)> {
        vec![
            ("mod.json".to_owned(), manifesto.as_bytes().to_vec()),
            ("cliente.js".to_owned(), b"SeeleUI;".to_vec()),
        ]
    }

    use seele_proto::mods::{MOD_API_MINIMA, MOD_API_VERSION};

    /// O manifesto de um pacote, na `api` dada: a deste build lida de
    /// `MOD_API_VERSION`, e nunca escrita (o guarda
    /// `nenhum_manifesto_de_teste_crava_a_versao_da_api`).
    fn manifesto(api: u32) -> String {
        format!(
            r#"{{"schema":1,"id":"ana/mesa","version":"1.0.0","api":{api},"repo":"https://exemplo.org/mesa","client":"cliente.js"}}"#
        )
    }

    #[test]
    fn o_pacote_guardado_no_lugar_do_proprio_hash_le_limpo_e_o_mexido_e_recusado() {
        let lugar = hash_do_conjunto(&mut pacote(&manifesto(MOD_API_VERSION)));
        let lido = guardado_em(pacote(&manifesto(MOD_API_VERSION)), &lugar);
        assert_eq!(
            (lido.id.as_str(), lido.refused, lido.hash.as_str()),
            ("ana/mesa", None, lugar.as_str())
        );

        let mut mexido = pacote(&manifesto(MOD_API_VERSION));
        mexido[1].1.push(b' ');
        let recusado = guardado_em(mexido, &lugar);
        assert_eq!(recusado.refused.as_deref(), Some("malformed-id"));
        // E diz o que ele é, para quem decide apagar.
        assert_eq!(
            (recusado.id.as_str(), recusado.version.as_str()),
            ("ana/mesa", "1.0.0")
        );
    }

    #[test]
    fn sem_manifesto_e_malformed_e_pelo_hash_confere_o_conteudo() {
        let recusado = guardado_em(vec![("cliente.js".to_owned(), Vec::new())], "abc");
        assert_eq!(
            (recusado.id.as_str(), recusado.refused.as_deref()),
            ("abc", Some("malformed"))
        );

        let lugar = hash_do_conjunto(&mut pacote(&manifesto(MOD_API_VERSION)));
        assert_eq!(
            pelo_hash(ler_pacote(&mut pacote(&manifesto(MOD_API_VERSION))), &lugar)
                .unwrap()
                .id,
            "ana/mesa"
        );
        assert_eq!(
            pelo_hash(
                ler_pacote(&mut pacote(&manifesto(MOD_API_VERSION))),
                "outro"
            ),
            Err("conteudo-nao-bate".to_owned())
        );
        // A API logo abaixo da mínima que este build aceita.
        let velho = manifesto(MOD_API_MINIMA - 1);
        assert_eq!(
            pelo_hash(ler_pacote(&mut pacote(&velho)), &lugar),
            Err("api-too-old".to_owned())
        );
    }
}
