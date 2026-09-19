//! A superfície de MODs que a casca enxerga.
//!
//! ADR 0045. Existe como camada e não como atalho: `xtask/src/check_deps.rs`
//! deixa a casca ver `seele-ffi` e nada além, e escreve o motivo —
//! «reaching past it would put protocol knowledge in a Tauri command»
//! (`specs/06-clientes-gui.md`). Sem este módulo, um comando do Tauri teria de
//! nomear `seele_core::mods::Found` e `seele_proto::mods::Refused`, que é
//! exatamente o vazamento que aquela regra transforma em build vermelho.
//!
//! O que ele faz, então, é o de sempre nesta fronteira: achatar em campos que
//! atravessam `serde` e substituir enum de domínio por **nome de recusa**, que a
//! casca vira frase (ADR 0012).

use seele_core::mods::{hex, refusal_name, Found};

/// A API de MODs que este build oferece — a **única** que ele oferece.
///
/// Reexportada porque a casca não alcança `seele-proto` nem `seele-core`
/// (ADR 0002 e 0039), e a alternativa é ela escrever o número à mão. O que uma segunda cópia deste
/// número custa já está registrado em
/// `crates/seele-conformance/tests/a_api_dos_mods_nao_diverge.rs`: a primeira
/// divergência só apareceu no primeiro MOD de verdade.
pub use seele_core::mods::MOD_API_VERSION;

/// Um MOD em disco, como a janela o desenha.
///
/// Um MOD recusado chega com `refused` preenchido em vez de ficar de fora da
/// lista: quem largou o diretório ali tem direito ao motivo, e o defeito que o
/// `CLAUDE.md` deste repositório nomeia como o mais caro é «o produto sabe e não
/// conta».
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModInstalado {
    /// `autor/nome`.
    pub id: String,
    /// A versão que o autor declara, ou vazio quando o manifesto não foi lido.
    pub version: String,
    /// O hash do conteúdo em hexadecimal, ou vazio. É o que uma pessoa compara
    /// a olho com o que o indexador publica.
    pub hash: String,
    /// O caminho do script que a janela carrega, se há metade de cliente.
    pub client: Option<String>,
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
    /// `api/v1.json` — rede de saída, relógio e log. A diferença entre isto e um
    /// MOD só de aparência é a diferença entre repintar uma janela e abrir
    /// conexões a partir da casa de alguém.
    pub server: bool,
    /// O nome da recusa, de uma lista fechada, quando houve uma.
    pub refused: Option<String>,
}

/// Onde os pacotes moram, endereçados pelo conteúdo — ver
/// [`seele_core::mods::PACOTES`].
pub const PACOTES: &str = seele_core::mods::PACOTES;

/// Todo pacote guardado, com o conteúdo conferido contra o nome da pasta.
#[must_use]
pub fn listar_por_conteudo(pasta: &str) -> Vec<ModInstalado> {
    seele_core::mods::listar_por_conteudo(std::path::Path::new(pasta))
        .into_iter()
        .map(achatar)
        .collect()
}

/// Lê um pacote pelo hash do conteúdo dele.
///
/// **Por hash e não por identificador**: dois servidores desta máquina podem
/// exigir bytes diferentes do mesmo MOD, e quem pergunta sabe quais quer.
///
/// # Errors
///
/// Devolve o nome da recusa quando o manifesto falta, não vale, ou quando o
/// conteúdo não é o que o nome da pasta promete.
pub fn ler_por_hash(pasta: &str, hash: &str) -> Result<ModInstalado, String> {
    let dir = std::path::Path::new(pasta).join(PACOTES).join(hash);
    match seele_core::mods::read_one(&dir) {
        Ok(instalado) if seele_core::mods::hex(&instalado.hash) == hash => Ok(ModInstalado {
            id: instalado.manifest.id,
            version: instalado.manifest.version,
            hash: hash.to_owned(),
            client: instalado.manifest.client,
            repo: instalado.manifest.repo,
            reach: instalado.manifest.reach,
            server: instalado.manifest.server.is_some(),
            refused: None,
        }),
        // O conteúdo não é o que o nome promete. Servir assim mesmo seria
        // entregar à janela bytes que ninguém revisou sob o hash de bytes que
        // alguém revisou — que é exatamente a promessa que o hash faz.
        Ok(_) => Err("conteudo-nao-bate".to_owned()),
        Err(motivo) => Err(refusal_name(&motivo).to_owned()),
    }
}

/// Todo MOD instalado nesta máquina, válido ou não.
///
/// Uma pasta `mods/` que não existe é uma lista vazia, e não um erro: é o estado
/// de toda instalação que nunca teve MOD.
#[must_use]
pub fn listar(pasta: &str) -> Vec<ModInstalado> {
    seele_core::mods::list(std::path::Path::new(pasta))
        .into_iter()
        .map(achatar)
        .collect()
}

/// Lê um MOD pelo identificador, para quem vai habilitá-lo.
///
/// # Errors
///
/// Devolve o nome da recusa quando o manifesto falta ou não vale.
pub fn ler_um(pasta: &str, id: &str) -> Result<ModInstalado, String> {
    let dir = std::path::Path::new(pasta).join("mods").join(id);
    match seele_core::mods::read_one(&dir) {
        Ok(instalado) => Ok(ModInstalado {
            id: instalado.manifest.id,
            version: instalado.manifest.version,
            hash: hex(&instalado.hash),
            client: instalado.manifest.client,
            repo: instalado.manifest.repo,
            reach: instalado.manifest.reach,
            server: instalado.manifest.server.is_some(),
            refused: None,
        }),
        Err(why) => Err(refusal_name(&why).to_owned()),
    }
}

/// Lê um pacote de MOD de uma pasta qualquer desta máquina.
///
/// Diferente de [`ler_um`], que procura dentro da pasta de MODs instalados:
/// aqui a pasta é a que a pessoa escolheu, e ainda não é instalação nenhuma.
///
/// Existe para a tela de instalar: ela precisa dizer **qual** MOD a pessoa
/// apontou, e recusar o que não serve, antes de qualquer byte ser copiado.
/// Copiar primeiro e conferir depois deixaria meia instalação em disco toda vez
/// que alguém apontasse para a pasta errada.
///
/// # Errors
///
/// Devolve o nome da recusa quando o manifesto falta ou não vale.
pub fn ler_pasta(caminho: &str) -> Result<ModInstalado, String> {
    match seele_core::mods::read_one(std::path::Path::new(caminho)) {
        Ok(instalado) => Ok(ModInstalado {
            id: instalado.manifest.id,
            version: instalado.manifest.version,
            hash: hex(&instalado.hash),
            client: instalado.manifest.client,
            repo: instalado.manifest.repo,
            reach: instalado.manifest.reach,
            server: instalado.manifest.server.is_some(),
            refused: None,
        }),
        Err(why) => Err(refusal_name(&why).to_owned()),
    }
}

/// O hash do conjunto de arquivos de um MOD, em hexadecimal minúsculo.
///
/// O **mesmo** número que o servidor anuncia, que a tela de aceite mostra e que
/// o catálogo do indexador publica — e é essa unicidade que faz a conferência
/// valer. Um segundo jeito de calcular «o hash de um MOD» seria um segundo
/// número para discordar do primeiro, e a pergunta que ele responde — «estes
/// bytes são os que foram revisados?» — não tolera duas respostas.
///
/// Toma `&mut` porque a ordenação acontece em quem digere: uma lista de
/// arquivos em ordem diferente tem de dar o mesmo hash em toda máquina.
#[must_use]
pub fn hash_do_conjunto(arquivos: &mut [(String, Vec<u8>)]) -> String {
    hex(&seele_core::mods::content_hash(arquivos))
}

/// Um achado do `seele-core` em campos que atravessam a fronteira.
fn achatar(found: Found) -> ModInstalado {
    match found {
        Found::Ok(instalado) => ModInstalado {
            id: instalado.manifest.id,
            version: instalado.manifest.version,
            hash: hex(&instalado.hash),
            client: instalado.manifest.client,
            repo: instalado.manifest.repo,
            reach: instalado.manifest.reach,
            server: instalado.manifest.server.is_some(),
            refused: None,
        },
        Found::Refused { id, why } => ModInstalado {
            id,
            version: String::new(),
            hash: String::new(),
            client: None,
            repo: String::new(),
            reach: Vec::new(),
            server: false,
            refused: Some(refusal_name(&why).to_owned()),
        },
    }
}

/// O caminho dentro de um MOD, reconstruído por componentes, ou nada.
///
/// Republicado e não reimplementado: a regra é a mesma que o servidor usa para
/// limitar um MOD à pasta dele, e duas cópias seriam dois lugares para
/// consertar e um para esquecer. Mora no `seele-proto` porque é o único crate
/// que a casca **e** o servidor alcançam — cada um pelo caminho que
/// `check_deps` permite.
///
/// # Errors
///
/// Devolve `None` para `..`, para componente absoluto, para raiz e para pedaço
/// vazio. Recusar em vez de resolver, porque resolver é onde um caminho que
/// parece contido deixa de estar.
#[must_use]
pub fn caminho_interno(partes: &[&str]) -> Option<std::path::PathBuf> {
    seele_core::mods::inner_path(partes)
}

// ------------------------------------------------- o aceite de quem entra
//
// ADR 0045: «aceitou uma vez, entra direto nas próximas». Guardar o sim é da
// casca, porque é ela que tem a tela onde ele é dado; **onde** ele é guardado é
// do núcleo, porque é o mesmo diretório de onde saem a identidade e os pins
// (ADR 0017).
//
// Estes três verbos existem porque sem eles o contrato não fecha: a casca recebe
// a lista pelo `ConnectionError::ModsNaoAceitos` e não teria como registrar a
// resposta — `check_deps` a deixa ver `seele-ffi` e nada além.

/// O que esta máquina já aceitou para este servidor, se algo.
///
/// `alvo` é o endereço **como a pessoa o digitou** ou como o convite o trouxe;
/// a forma canônica sob a qual ele é arquivado sai de
/// [`crate::chave_do_servidor`], e não da casca — ver o porquê ali.
#[must_use]
pub fn aceite_de(home: &str, alvo: &str) -> Option<String> {
    let chave = crate::chave_do_servidor(alvo)?;
    seele_core::aceites::Aceites::em(std::path::Path::new(home)).aceito_de(&chave)
}

/// Um aceite guardado, como a tela o desenha.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AceiteGuardado {
    /// Sob que chave o servidor está arquivado. A mesma do pin.
    pub alvo: String,
    /// A identidade do conjunto aceito.
    pub conjunto: String,
    /// Quando, em segundos desde a época.
    pub quando: i64,
}

/// Todo sim que esta máquina já deu, para a tela poder desfazê-los.
///
/// Existe porque a promessa do ADR 0045 — «um consentimento que não se retira
/// não é consentimento» — precisa de um lugar onde a pessoa **veja** o que
/// aceitou. `esquecer_aceite` já existia e só podia ser chamada por quem já
/// soubesse o endereço de cor.
#[must_use]
pub fn aceites_guardados(home: &str) -> Vec<AceiteGuardado> {
    seele_core::aceites::Aceites::em(std::path::Path::new(home))
        .listar()
        .into_iter()
        .map(|aceite| AceiteGuardado {
            alvo: aceite.alvo,
            conjunto: aceite.conjunto,
            quando: aceite.quando,
        })
        .collect()
}

/// Guarda o sim que a pessoa acabou de dar a um conjunto de MODs.
///
/// A entrada seguinte neste servidor passa direto, e só neste — um aceite é de
/// um servidor, e não desta máquina.
///
/// # Errors
///
/// Devolve `"NaoDeuParaGravar"` quando o arquivo não pôde ser escrito, e
/// `"EnderecoInvalido"` quando o alvo não é um endereço. Nomes de recusa e não
/// frases, como as vizinhas: a fronteira erro→texto é do frontend.
///
/// Recusar um alvo inválido em vez de gravá-lo como veio é o que impede o modo
/// de falha mudo: um aceite arquivado sob uma chave que ninguém lê é um sim que
/// a pessoa deu e que o produto vai pedir de novo em toda entrada.
pub fn aceitar(home: &str, alvo: &str, conjunto: &str) -> Result<(), String> {
    let chave = crate::chave_do_servidor(alvo).ok_or("EnderecoInvalido")?;
    seele_core::aceites::Aceites::em(std::path::Path::new(home))
        .aceitar_agora(&chave, conjunto)
        .map_err(|_| "NaoDeuParaGravar".to_owned())
}

/// Desfaz o sim dado a este servidor.
///
/// Existe porque um consentimento que não se retira não é consentimento: a
/// entrada seguinte volta a mostrar a lista.
///
/// # Errors
///
/// Devolve `"NaoDeuParaGravar"` quando o arquivo não pôde ser escrito, e
/// `"EnderecoInvalido"` quando o alvo não é um endereço.
pub fn esquecer_aceite(home: &str, alvo: &str) -> Result<(), String> {
    let chave = crate::chave_do_servidor(alvo).ok_or("EnderecoInvalido")?;
    seele_core::aceites::Aceites::em(std::path::Path::new(home))
        .esquecer(&chave)
        .map_err(|_| "NaoDeuParaGravar".to_owned())
}

#[cfg(test)]
mod tests_do_aceite {
    use super::*;

    fn pasta(nome: &str) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static QUAL: AtomicUsize = AtomicUsize::new(0);
        let n = QUAL.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seele-ffi-aceites-{nome}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir.to_string_lossy().into_owned()
    }

    /// O caminho inteiro que a casca percorre: não há aceite, ela guarda um, e
    /// a entrada seguinte o encontra.
    #[test]
    fn a_casca_guarda_o_aceite_e_o_encontra_na_entrada_seguinte() {
        let home = pasta("guarda");
        let conjunto = "a1".repeat(32);
        assert_eq!(aceite_de(&home, "casa:8383"), None);

        aceitar(&home, "casa:8383", &conjunto).expect("guardar");
        assert_eq!(aceite_de(&home, "casa:8383"), Some(conjunto));

        esquecer_aceite(&home, "casa:8383").expect("esquecer");
        assert_eq!(aceite_de(&home, "casa:8383"), None);
    }
}
