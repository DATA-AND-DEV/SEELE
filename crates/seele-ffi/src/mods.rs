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
pub use seele_core::mods::{
    capacidades_da_api, capacidades_da_maquina, capacidades_de, capacidades_do_manifesto, Poder,
    APIS_ACEITAS, MOD_API_MINIMA, MOD_API_VERSION,
};

/// Ler um arquivo de mídia de um MOD — ver [`seele_core::mods::ler_midia`].
///
/// Reexportado e não reescrito: a casca não alcança `seele-core` (ADR 0002), e
/// o `data:` precisa ser composto onde o tipo é decidido pelos bytes.
pub use seele_core::mods::{
    ler_midia, ler_tipo, reconhecido_e_nao_aceito, MidiaDeMod, TipoLido, TETO_DE_MIDIA,
};

/// Um MOD em disco, como a janela o desenha: o do `seele-sessao`, que o
/// navegador monta igual (Tarefa 2.6 da F4). Ver
/// [`seele_core::mods::ModInstalado`].
pub use seele_core::mods::ModInstalado;

/// O arquivo que um MOD serve, pela regra que o navegador usa também — ver
/// [`seele_core::mods::caminho_declarado`].
pub use seele_core::mods::caminho_declarado;

/// Onde os pacotes moram, endereçados pelo conteúdo — ver
/// [`seele_core::mods::PACOTES`].
pub const PACOTES: &str = seele_core::mods::PACOTES;

/// O diretório de um pacote no cache, se o hash tem forma de hash.
///
/// A conferência de forma é o que impede a travessia: o hash chega da janela.
/// Ver [`seele_core::mods::caminho_do_pacote`].
#[must_use]
pub fn caminho_do_pacote(pasta: &str, hash: &str) -> Option<std::path::PathBuf> {
    seele_core::mods::caminho_do_pacote(std::path::Path::new(pasta), hash)
}

/// Quantos bytes um pacote ocupa no cache, ou zero quando ele não está lá.
#[must_use]
pub fn bytes_do_pacote(pasta: &str, hash: &str) -> u64 {
    seele_core::mods::bytes_do_pacote(std::path::Path::new(pasta), hash)
}

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
    // A decisão é a do navegador também: o conteúdo que não é o que o nome
    // promete não é servido (`conteudo-nao-bate`).
    seele_core::mods::pelo_hash(
        seele_core::mods::read_one(&dir).map(|lido| (lido.manifest, lido.hash)),
        hash,
    )
}

/// **A metade de máquina de um pacote** (API 8 parte 3): o fonte do arquivo
/// que `manifesto.maquina` nomeia e o que ela pode chamar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadeDeMaquina {
    /// `autor/nome`, do manifesto dos bytes lidos.
    pub id: String,
    /// O texto do arquivo.
    pub fonte: String,
    /// [`capacidades_da_maquina`] do mesmo manifesto.
    pub capacidades: Vec<&'static str>,
}

/// Lê a metade de máquina do pacote de conteúdo `hash`.
///
/// **Só quem aceitou chama isto**, e no instante do aceite: o código da
/// metade de máquina não é lido antes de o dono da máquina dizer sim. Por
/// isso ela não passa por [`caminho_declarado`] — esse é o que a janela pode
/// servir, e a metade de máquina **não** é servida à janela nunca.
///
/// O manifesto, o hash e o arquivo saem da mesma leitura da pasta: quem
/// decide o que ela pode chamar é o mesmo manifesto que provou os bytes.
///
/// # Errors
///
/// `conteudo-nao-bate` quando o conteúdo não é o que o hash promete, o nome
/// da recusa do manifesto, `sem-metade-de-maquina`, `maquina-fora-do-pacote`,
/// `maquina-nao-lida` ou `maquina-nao-e-utf8`.
pub fn codigo_da_maquina(pasta: &str, hash: &str) -> Result<MetadeDeMaquina, String> {
    let dir = std::path::Path::new(pasta).join(PACOTES).join(hash);
    // **Os bytes da metade saem da mesma leitura que deu o hash**: ler o
    // arquivo de novo depois de conferir seria conferir uns bytes e rodar
    // outros, se alguém trocasse o arquivo no meio.
    let (manifest, conteudo, arquivos) =
        seele_core::mods::read_with_files(&dir).map_err(|why| refusal_name(&why).to_owned())?;
    if hex(&conteudo) != hash {
        return Err("conteudo-nao-bate".to_owned());
    }
    let Some(caminho) = manifest.maquina.as_deref() else {
        return Err("sem-metade-de-maquina".to_owned());
    };
    // O manifesto já foi recusado se o caminho sai do pacote; conferir de novo
    // aqui custa uma linha e não depende de que aquela regra continue lá.
    let relativo = seele_core::mods::inner_path(&caminho.split('/').collect::<Vec<_>>())
        .ok_or_else(|| "maquina-fora-do-pacote".to_owned())?;
    let bytes = arquivos
        .into_iter()
        .find(|(nome, _)| std::path::Path::new(nome) == relativo.as_path())
        .map(|(_, bytes)| bytes)
        .ok_or_else(|| "maquina-nao-lida".to_owned())?;
    let fonte = String::from_utf8(bytes).map_err(|_| "maquina-nao-e-utf8".to_owned())?;
    Ok(MetadeDeMaquina {
        id: manifest.id.clone(),
        fonte,
        capacidades: capacidades_da_maquina(&manifest),
    })
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
        Ok(instalado) => Ok(ModInstalado::lido(instalado.manifest, hex(&instalado.hash))),
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
        Ok(instalado) => Ok(ModInstalado::lido(instalado.manifest, hex(&instalado.hash))),
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
pub use seele_core::mods::hash_do_conjunto;

/// Um achado do `seele-core` em campos que atravessam a fronteira.
fn achatar(found: Found) -> ModInstalado {
    match found {
        Found::Ok(instalado) => ModInstalado::lido(instalado.manifest, hex(&instalado.hash)),
        Found::Refused { id, why, diz_ser } => ModInstalado::recusado(id, &why, diz_ser),
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

// ------------------------------------------- o aceite de uma máquina de jogo
//
// API 8 parte 3. Como o aceite de MODs logo acima: guardar é da casca, que tem
// a tela; onde se guarda é do núcleo (`seele_core::aceites_de_maquina`), no
// mesmo diretório da identidade. A chave do servidor sai de
// `chave_do_servidor`, pelo mesmo motivo: um aceite arquivado sob a forma que a
// casca digitou seria um sim que ninguém acha.

/// Um aceite de máquina guardado, como a casca o lê e o devolve.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AceiteDeMaquinaGuardado {
    /// Sob que chave o servidor SEELE está arquivado.
    pub alvo: String,
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O hash do pacote ligado quando a pessoa aceitou.
    pub hash: String,
    /// O servidor de jogo.
    pub servidor: String,
    /// O resumo do que foi mostrado, como o [`crate::Event::MachineRequest`]
    /// o trouxe.
    pub resumo: String,
    /// A oferta que a pessoa leu.
    pub oferta: crate::OfertaDeMaquinaFfi,
    /// O apelido de quem pediu.
    pub pedido_por: String,
    /// Quando, em segundos desde a época.
    pub quando: u64,
}

impl AceiteDeMaquinaGuardado {
    fn de(aceite: seele_core::aceites_de_maquina::AceiteDeMaquina) -> Self {
        Self {
            oferta: crate::OfertaDeMaquinaFfi::from(&aceite.oferta),
            alvo: aceite.alvo,
            mod_id: aceite.mod_id,
            hash: aceite.hash,
            servidor: aceite.servidor,
            resumo: aceite.resumo,
            pedido_por: aceite.pedido_por,
            quando: aceite.quando,
        }
    }
}

/// Os aceites de máquina legíveis, e quantas linhas não se leram.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AceitesDeMaquina {
    /// Os que se leram.
    pub aceites: Vec<AceiteDeMaquinaGuardado>,
    /// Quantas linhas foram puladas. Diferente de zero, vai ao log da casca:
    /// é um aceite que a pessoa deu e que o produto não acha mais.
    pub ilegiveis: usize,
}

fn repositorio(home: &str) -> seele_core::aceites_de_maquina::AceitesDeMaquina {
    seele_core::aceites_de_maquina::AceitesDeMaquina::em(std::path::Path::new(home))
}

/// Todo aceite de máquina desta instalação.
///
/// # Errors
///
/// `"NaoDeuParaLer"` quando o arquivo existe e não se lê.
pub fn aceites_de_maquina(home: &str) -> Result<AceitesDeMaquina, String> {
    let repositorio = repositorio(home);
    let aceites = repositorio
        .listar()
        .map_err(|_| "NaoDeuParaLer".to_owned())?
        .into_iter()
        .map(AceiteDeMaquinaGuardado::de)
        .collect();
    Ok(AceitesDeMaquina {
        aceites,
        ilegiveis: repositorio.ilegiveis(),
    })
}

/// O aceite desta máquina, se houver. `alvo` como a pessoa o digitou.
///
/// # Errors
///
/// `"EnderecoInvalido"` e `"NaoDeuParaLer"`.
pub fn aceite_de_maquina(
    home: &str,
    alvo: &str,
    mod_id: &str,
    servidor: &str,
) -> Result<Option<AceiteDeMaquinaGuardado>, String> {
    let chave = crate::chave_do_servidor(alvo).ok_or("EnderecoInvalido")?;
    Ok(repositorio(home)
        .de(&chave, mod_id, servidor)
        .map_err(|_| "NaoDeuParaLer".to_owned())?
        .map(AceiteDeMaquinaGuardado::de))
}

/// Guarda o sim da pessoa, no lugar do anterior da mesma máquina. `quando` é
/// o relógio desta máquina, e `alvo` vira a chave canônica antes de ir ao
/// disco — o `alvo` do `aceite` que chega é ignorado.
///
/// # Errors
///
/// `"EnderecoInvalido"`, `"OfertaInvalida"` (uma oferta que o fio recusaria
/// não vira aceite), `"ResumoNaoConfere"` (o resumo não é o desta oferta, deste
/// pacote e deste servidor de jogo) e `"NaoDeuParaGravar"`.
pub fn guardar_aceite_de_maquina(
    home: &str,
    alvo: &str,
    aceite: &AceiteDeMaquinaGuardado,
) -> Result<(), String> {
    let chave = crate::chave_do_servidor(alvo).ok_or("EnderecoInvalido")?;
    let oferta = aceite
        .oferta
        .para_o_nucleo()
        .map_err(|_| "OfertaInvalida".to_owned())?;
    // O resumo é o que a pessoa aceitou; a oferta guardada é o que a parte 4
    // executa. Se os dois não falam da mesma coisa — uma casca que remontou a
    // oferta errado, ou juntou a oferta de um pedido ao resumo de outro —, o
    // disco guardaria permissão para algo que ninguém viu.
    let resumo = seele_core::maquina::resumo_em_hex(&seele_core::maquina::resumo_da_oferta(
        &aceite.mod_id,
        &aceite.hash,
        &aceite.servidor,
        &oferta,
    ));
    if resumo != aceite.resumo {
        return Err("ResumoNaoConfere".to_owned());
    }
    let quando = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |passou| passou.as_secs());
    repositorio(home)
        .guardar(&seele_core::aceites_de_maquina::AceiteDeMaquina {
            alvo: chave,
            mod_id: aceite.mod_id.clone(),
            hash: aceite.hash.clone(),
            servidor: aceite.servidor.clone(),
            resumo: aceite.resumo.clone(),
            oferta,
            pedido_por: aceite.pedido_por.clone(),
            quando,
        })
        .map_err(|_| "NaoDeuParaGravar".to_owned())
}

/// Apaga o aceite desta máquina. `true` se havia um.
///
/// # Errors
///
/// `"EnderecoInvalido"` e `"NaoDeuParaGravar"`.
pub fn revogar_aceite_de_maquina(
    home: &str,
    alvo: &str,
    mod_id: &str,
    servidor: &str,
) -> Result<bool, String> {
    let chave = crate::chave_do_servidor(alvo).ok_or("EnderecoInvalido")?;
    repositorio(home)
        .revogar(&chave, mod_id, servidor)
        .map_err(|_| "NaoDeuParaGravar".to_owned())
}

/// Onde mora a pasta deste servidor de jogo (Desvio 10). Não cria nada.
///
/// # Errors
///
/// `"EnderecoInvalido"`.
pub fn pasta_do_servidor_de_jogo(
    home: &str,
    alvo: &str,
    mod_id: &str,
    servidor: &str,
) -> Result<std::path::PathBuf, String> {
    let chave = crate::chave_do_servidor(alvo).ok_or("EnderecoInvalido")?;
    Ok(seele_core::aceites_de_maquina::pasta_do_servidor_de_jogo(
        std::path::Path::new(home),
        &chave,
        mod_id,
        servidor,
    ))
}

#[cfg(test)]
mod tests_do_aceite_de_maquina {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn pasta(nome: &str) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static QUAL: AtomicUsize = AtomicUsize::new(0);
        let n = QUAL.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seele-ffi-maquinas-{nome}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir.to_string_lossy().into_owned()
    }

    fn aceite() -> AceiteDeMaquinaGuardado {
        let mut aceite = sem_resumo();
        aceite.resumo = seele_core::maquina::resumo_em_hex(&seele_core::maquina::resumo_da_oferta(
            &aceite.mod_id,
            &aceite.hash,
            &aceite.servidor,
            &aceite.oferta.para_o_nucleo().unwrap(),
        ));
        aceite
    }

    fn sem_resumo() -> AceiteDeMaquinaGuardado {
        AceiteDeMaquinaGuardado {
            alvo: String::new(),
            mod_id: "seele/jogo".into(),
            hash: "ab".repeat(32),
            servidor: "mundo".into(),
            resumo: "cd".repeat(32),
            oferta: crate::OfertaDeMaquinaFfi {
                titulo: "Minecraft".into(),
                receita: "minecraft-java".into(),
                versao_da_receita: "1.0.0".into(),
                versao_do_jogo: "1.21.4".into(),
                tuneis: vec![crate::TunelOferecidoFfi {
                    nome: "mc".into(),
                    protocolo: "tcp".into(),
                    porta: 25565,
                }],
                artefatos: vec![],
                pack: None,
                origens: vec![],
                dominios_do_jogo: vec![],
                rodar: None,
                disco_maximo: 0,
                termos: vec![],
            },
            pedido_por: "marcela".into(),
            quando: 0,
        }
    }

    /// O caminho da casca: guarda pelo endereço digitado, acha pela chave
    /// canônica, revoga, e a pasta do jogo nunca é a de configuração.
    #[test]
    fn a_casca_guarda_acha_e_revoga_pela_chave_do_servidor() {
        let home = pasta("ciclo");
        assert_eq!(
            aceite_de_maquina(&home, "casa", "seele/jogo", "mundo"),
            Ok(None)
        );
        guardar_aceite_de_maquina(&home, "casa", &aceite()).unwrap();

        let achado = aceite_de_maquina(&home, "casa:8383", "seele/jogo", "mundo")
            .unwrap()
            .expect("o aceite guardado pelo endereço curto");
        assert_eq!(achado.alvo, crate::chave_do_servidor("casa").unwrap());
        assert_eq!(achado.oferta, aceite().oferta);
        assert!(achado.quando > 0);
        assert_eq!(aceites_de_maquina(&home).unwrap().aceites.len(), 1);

        let pasta_do_jogo =
            pasta_do_servidor_de_jogo(&home, "casa", "seele/jogo", "mundo").unwrap();
        assert_ne!(pasta_do_jogo, std::path::PathBuf::from(&home));

        assert_eq!(
            revogar_aceite_de_maquina(&home, "casa", "seele/jogo", "mundo"),
            Ok(true)
        );
        assert_eq!(
            aceite_de_maquina(&home, "casa", "seele/jogo", "mundo"),
            Ok(None)
        );
    }

    /// O resumo guardado é o da oferta guardada: com outra porta, outro
    /// pacote ou um resumo qualquer, nada vai ao disco.
    #[test]
    fn um_resumo_que_nao_e_o_da_oferta_nao_vira_aceite() {
        let home = pasta("resumo");
        let mut outra_porta = aceite();
        outra_porta.oferta.tuneis[0].porta = 25566;
        let mut outro_pacote = aceite();
        outro_pacote.hash = "ef".repeat(32);
        for torto in [sem_resumo(), outra_porta, outro_pacote] {
            assert_eq!(
                guardar_aceite_de_maquina(&home, "casa", &torto),
                Err("ResumoNaoConfere".to_owned())
            );
        }
        assert_eq!(aceites_de_maquina(&home).unwrap().aceites.len(), 0);
        guardar_aceite_de_maquina(&home, "casa", &aceite()).unwrap();
    }

    #[test]
    fn uma_oferta_torta_nao_vira_aceite() {
        let home = pasta("torta");
        let mut torta = aceite();
        torta.oferta.tuneis[0].protocolo = "quic".into();
        assert_eq!(
            guardar_aceite_de_maquina(&home, "casa", &torta),
            Err("OfertaInvalida".to_owned())
        );
        assert_eq!(aceites_de_maquina(&home).unwrap().aceites.len(), 0);
    }
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

#[cfg(test)]
mod tests_da_porta_do_tema {
    use super::*;

    /// Um pacote numa pasta temporária, com a porta do tema ou sem ela.
    fn pacote(nome: &str, tema: bool) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static QUAL: AtomicUsize = AtomicUsize::new(0);
        let n = QUAL.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seele-ffi-porta-do-tema-{nome}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("servidor")).expect("temporário");
        std::fs::write(
            dir.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"seele/{nome}","version":"1.0.0","api":{MOD_API_VERSION},"repo":"https://github.com/seele/{nome}","reach":[],"server":"servidor/main.js","tema":{tema}}}"#
            ),
        )
        .expect("manifesto");
        std::fs::write(
            dir.join("servidor/main.js"),
            "globalThis.aoPedir = () => '';",
        )
        .expect("metade de servidor");
        dir
    }

    /// **A gestão sabe quem tem a porta do tema** (tarefa 17f.0 da F3): é com
    /// isto que ela pergunta qual MOD pinta o servidor quando dois com a porta
    /// estão ligados. Pelos três caminhos que a casca usa — a pasta escolhida,
    /// o pacote por hash e a lista do cache —, e fechada sem a chave.
    #[test]
    fn a_porta_do_tema_chega_a_quem_administra_e_fica_fechada_sem_a_chave() {
        let home = std::env::temp_dir().join(format!(
            "seele-ffi-porta-do-tema-home-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let home_texto = home.to_string_lossy().into_owned();

        for (nome, tema) in [("estilo", true), ("perfis", false)] {
            let dir = pacote(nome, tema);
            let lido = ler_pasta(&dir.to_string_lossy()).expect("pacote válido recusado");
            assert_eq!(lido.tema, tema, "{nome}: a pasta escolhida");

            let guardado = home.join(PACOTES).join(&lido.hash);
            std::fs::create_dir_all(guardado.join("servidor")).expect("cache");
            for arquivo in ["mod.json", "servidor/main.js"] {
                std::fs::copy(dir.join(arquivo), guardado.join(arquivo)).expect("copiar");
            }
            let por_hash = ler_por_hash(&home_texto, &lido.hash).expect("pacote por hash");
            assert_eq!(por_hash.tema, tema, "{nome}: o pacote por hash");
        }

        let lista = listar_por_conteudo(&home_texto);
        let porta = |id: &str| lista.iter().find(|m| m.id == id).map(|m| m.tema);
        assert_eq!(porta("seele/estilo"), Some(true), "a lista do cache");
        assert_eq!(porta("seele/perfis"), Some(false), "a lista do cache");
    }
}

#[cfg(test)]
mod hash_de_pasta {
    /// Instrumento de homologação: imprime o hash de conteúdo de uma pasta.
    ///
    /// Não é um teste do produto — é a forma de descobrir, com o **mesmo
    /// código** que o servidor usa, onde um pacote precisa ficar para ser
    /// encontrado. Adivinhar o caminho é a maneira mais rápida de homologar
    /// nada e achar que homologou.
    #[test]
    #[ignore = "instrumento: SEELE_PASTA=<pasta> cargo test -p seele-ffi hash_de_pasta -- --ignored --nocapture"]
    fn imprime() {
        let Ok(pasta) = std::env::var("SEELE_PASTA") else {
            panic!("SEELE_PASTA não está definida");
        };
        let instalado = super::ler_pasta(&pasta).expect("ler a pasta");
        println!(
            "HASH={} ID={} VERSAO={}",
            instalado.hash, instalado.id, instalado.version
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod a_metade_de_maquina_do_pacote {
    use super::*;

    /// Um pacote de API 8 com metade de máquina, publicado pelo conteúdo como o
    /// instalador faz. Devolve a pasta de configuração e o hash.
    fn publicar(nome: &str) -> (String, String) {
        let raiz =
            std::env::temp_dir().join(format!("seele-ffi-metade-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        let dir = raiz.join("em-obras");
        std::fs::create_dir_all(dir.join("cliente")).unwrap();
        std::fs::create_dir_all(dir.join("maquina")).unwrap();
        std::fs::write(
            dir.join("mod.json"),
            r#"{"schema": 1, "id": "x/jogo", "version": "1.0.0", "api": 8,
                "repo": "https://github.com/x/jogo", "reach": ["dom"], "state": 1,
                "client": "cliente/main.js", "plataformas": ["app"], "poderes": ["porta"],
                "maquina": "maquina/main.js", "server": "servidor.js",
                "permissoes": [{"acao": "entrar-no-jogo", "nome": "Entrar no jogo", "todos": true}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("cliente/main.js"), "// janela").unwrap();
        std::fs::write(dir.join("maquina/main.js"), "// máquina").unwrap();
        std::fs::write(dir.join("servidor.js"), "// servidor").unwrap();
        let hash = ler_pasta(&dir.to_string_lossy())
            .expect("o pacote é válido")
            .hash;
        let destino = raiz.join(PACOTES).join(&hash);
        std::fs::create_dir_all(destino.parent().unwrap()).unwrap();
        std::fs::rename(&dir, &destino).unwrap();
        (raiz.to_string_lossy().into_owned(), hash)
    }

    /// **O arquivo de `manifesto.maquina`, com as capacidades da máquina** — e
    /// não as da janela, que dariam a ela um `SeeleUI`.
    #[test]
    fn a_metade_de_maquina_sai_do_arquivo_que_o_manifesto_nomeia() {
        let (pasta, hash) = publicar("le");
        let metade = codigo_da_maquina(&pasta, &hash).expect("a metade de máquina");
        assert_eq!(metade.id, "x/jogo");
        assert_eq!(metade.fonte, "// máquina");
        assert_eq!(metade.capacidades, vec!["maquina", "maquina.porta"]);
        // Outro hash é outro conteúdo, e o que não bate não é lido.
        let outro = "0".repeat(hash.len());
        assert!(codigo_da_maquina(&pasta, &outro).is_err());
    }
}
