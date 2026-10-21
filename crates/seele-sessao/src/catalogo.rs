//! **O catálogo de MODs, do lado de quem baixa** — ADR 0045 —, sem a rede
//! (Tarefa 2.6 da F4): o desktop e o navegador conferem, leem, planejam e
//! recusam pelas mesmas funções. A rede é de cada casca (o `reqwest` do
//! desktop, o `fetch` da página), e o disco também.
//!
//! # A ordem, e por que ela é esta
//!
//! Assinatura **antes** de analisar. Um JSON que ainda não foi conferido é
//! texto de terceiro, e analisá-lo primeiro é escolher confiar nele para
//! decidir se se confia nele. O `indexador-de-mods.md` é literal: *«confere a
//! assinatura contra a chave embutida — se falhar, para, e não tenta de novo
//! com outro endereço»*.
//!
//! # Por que os arquivos vêm soltos, e por que isso é bom aqui
//!
//! O catálogo publica `mods/<autor>/<nome>/<versao>/<arquivo>`, e não um
//! `.zip`. O cliente não descobre nada: ele lê a lista `arquivos` da versão,
//! busca exatamente aquilo, e confere o **hash do conjunto** — o mesmo
//! `content_hash` que o servidor anuncia e que a tela de aceite mostra. Não há
//! formato de arquivo a escolher, não há extração, e não há *zip slip*.
//!
//! # A chave vem de quem chama
//!
//! Cada casca compila a mesma `apps/seele-app/chaves/mods.pub` para dentro de
//! si e a entrega aqui. Nunca uma chave que venha junto do arquivo: uma chave
//! que chega pela mesma porta que o conteúdo não prova nada sobre ele.

use serde::Deserialize;

/// Onde o catálogo mora.
///
/// Uma constante e não configuração: um endereço que a pessoa pudesse trocar
/// seria um endereço que alguém troca por ela. A assinatura protege o conteúdo;
/// o que protege a **origem** é não haver nada a apontar para outro lugar.
pub const ENDERECO: &str = "https://mods.seele.app.br";

/// O esquema de catálogo que este build lê.
pub const ESQUEMA_CONHECIDO: u32 = 1;

/// Por que o catálogo não pôde ser usado.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum FalhaNoCatalogo {
    /// Não respondeu, ou respondeu erro.
    NaoRespondeu(String),
    /// **A assinatura do catálogo não confere.** Não se tenta de novo, e não se
    /// tenta outro endereço: um catálogo que não passa é um catálogo adulterado
    /// ou um endereço sequestrado, e insistir é o comportamento que o atacante
    /// quer.
    ///
    /// **O nome é longo de propósito.** `AssinaturaRecusada` já é de outra
    /// coisa — o pacote do atualizador, no `FRASES` do JavaScript —, e as duas
    /// pedem conselhos diferentes. Os dois nomes colidiram uma vez, calados,
    /// num objeto literal onde a última chave vence: a frase que a pessoa lia
    /// era a do atualizador, falando de um pacote que ninguém tinha baixado.
    AssinaturaDoCatalogoRecusada,
    /// A assinatura passou e o conteúdo não é um catálogo que este build lê.
    NaoEUmCatalogo(String),
    /// O esquema é mais novo que este build.
    ///
    /// Separado de [`Self::NaoEUmCatalogo`] porque o conserto é outro: aqui não
    /// há nada errado com o catálogo — quem está velho é este SEELE.
    MaisNovoQueEsteBuild {
        /// O que o arquivo pede.
        pedido: u32,
        /// O que este build lê.
        conhecido: u32,
    },
    /// Os bytes chegaram e a exigência do servidor não acompanhou.
    ///
    /// QA-01 de 17/09. Atualizar um MOD que estava ligado troca os arquivos no
    /// disco, e a linha que o servidor guarda — a que carrega o hash e vai no
    /// anúncio — continua descrevendo a versão anterior. Reaplicar fecha a
    /// operação; quando **isso** falha, o desfecho é próprio e merece nome
    /// próprio: o pacote novo está instalado e o servidor ainda pede o velho.
    NaoAplicou(String),
    /// Um MOD ou uma versão que o catálogo não lista.
    NaoEstaNoCatalogo(String),
    /// Os bytes baixados não são os que o catálogo declara.
    ///
    /// Separado da assinatura de propósito, e as respostas são opostas: aqui a
    /// causa provável é um download truncado e a coisa a fazer é tentar de
    /// novo; ali é **não** tentar de novo.
    ConteudoDivergente {
        /// O que o catálogo diz.
        esperado: String,
        /// O que chegou.
        obtido: String,
    },
    /// Esta versão foi revogada, e por quê.
    Revogada {
        /// O identificador do defeito, da lista fechada do indexador.
        motivo: String,
        /// Em que versão ele foi corrigido, quando a lista diz.
        corrigido_em: Option<String>,
    },
    /// **O catálogo entregou os bytes certos, e esta máquina não os instalou**:
    /// o disco (ou o armazenamento da página) recusou, ou o `mod.json` não
    /// serve a este SEELE. Com a recusa de instalar dentro, e a tela diz a
    /// frase dela (pendência 58, item 9).
    ///
    /// Separado de [`Self::NaoEUmCatalogo`] e de [`Self::NaoRespondeu`] porque
    /// os conselhos se opõem: a falha é daqui, e não de quem publica nem da
    /// rede.
    NaoInstalou(FalhaAoInstalarMod),
}

/// Por que um pacote não pôde ser instalado.
///
/// Uma variante por recusa, com o motivo dentro — a mesma regra do resto da
/// ponte: a frase mora no `FRASES` do JavaScript, e ela precisa saber **qual**
/// recusa foi para dizer o conserto.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum FalhaAoInstalarMod {
    /// A pasta escolhida (ou o conjunto baixado) não tem `mod.json`.
    ///
    /// O engano mais provável de todos no desktop: apontar para a pasta que
    /// **contém** o MOD em vez da pasta do MOD.
    SemManifesto,
    /// O `mod.json` existe e não vale, com o nome da recusa
    /// ([`crate::mods::refusal_name`]).
    ManifestoRecusado(String),
    /// Há um atalho (link simbólico) dentro da pasta, com o caminho dele. Só o
    /// desktop o encontra: a página não tem atalho.
    ///
    /// **A recusa está certa, e a variante própria é para dizê-la.** O caso que
    /// a separou de [`Self::NaoCopiei`] é a pasta de quem desenvolve o MOD,
    /// com `node_modules/.bin/` cheia de atalhos: misturada às falhas do disco,
    /// a tela não tinha como dizer qual era o atalho nem o que fazer
    /// (pendência 53).
    TemAtalho(String),
    /// O disco (ou o armazenamento da página) recusou, e o que ele disse.
    NaoCopiei(String),
}

/// O catálogo inteiro.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Catalogo {
    /// A versão do formato.
    pub esquema: u32,
    /// Quando foi gerado, em segundos desde a época.
    #[serde(default)]
    pub gerado_em: i64,
    /// Um por MOD.
    #[serde(default)]
    pub mods: Vec<ModNoCatalogo>,
}

/// Um MOD, como o catálogo o lista.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ModNoCatalogo {
    /// `autor/nome`.
    pub id: String,
    /// O nome legível.
    #[serde(default)]
    pub titulo: String,
    /// Uma linha sobre o que ele faz.
    #[serde(default)]
    pub resumo: String,
    /// O repositório público.
    #[serde(default)]
    pub repo: String,
    /// O veredito mais recente, para listar e filtrar.
    ///
    /// **Não carimba a versão**: quem carimba um número de versão é o `nivel`
    /// de dentro de `versoes[]`, porque o veredito ao lado de um hash tem de
    /// ser o veredito daqueles bytes. O `indexador-de-mods.md` é explícito.
    #[serde(default)]
    pub nivel: String,
    /// As versões publicadas, da mais antiga para a mais nova.
    #[serde(default)]
    pub versoes: Vec<VersaoNoCatalogo>,
}

/// Uma versão publicada de um MOD.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct VersaoNoCatalogo {
    /// O número que o autor declara.
    pub versao: String,
    /// Contra que API do MOD ela foi escrita.
    #[serde(default)]
    pub api: u32,
    /// O hash do conjunto, em hexadecimal minúsculo.
    pub hash: String,
    /// O veredito **destes bytes**.
    #[serde(default)]
    pub nivel: String,
    /// Os avisos do terceiro nível, como identificadores.
    #[serde(default)]
    pub notas: Vec<String>,
    /// O que esta versão declara alcançar.
    #[serde(default)]
    pub alcanca: Vec<String>,
    /// A lista exata de arquivos a buscar.
    ///
    /// **O cliente não descobre nada.** Ele lê esta lista, busca aquilo, e
    /// confere o hash do conjunto — um catálogo adulterado que omitisse um
    /// arquivo mudaria o hash, e a conferência o pega.
    #[serde(default)]
    pub arquivos: Vec<String>,
}

/// Uma revogação: uma versão que saiu de circulação, e por quê.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Revogacao {
    /// Qual MOD.
    pub id: String,
    /// Qual versão.
    pub versao: String,
    /// O identificador do defeito, da lista fechada.
    #[serde(default)]
    pub motivo: String,
    /// Em que versão ele foi corrigido, quando há uma.
    #[serde(default)]
    pub corrigido_em: Option<String>,
}

/// A lista de revogações inteira.
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct Revogacoes {
    /// A versão do formato.
    #[serde(default)]
    pub esquema: u32,
    /// As versões de MOD retiradas.
    #[serde(default)]
    pub mods: Vec<Revogacao>,
}

/// Confere a assinatura e só então analisa.
///
/// # Errors
///
/// [`FalhaNoCatalogo::AssinaturaDoCatalogoRecusada`] quando a assinatura não
/// bate — e **quem chama não tenta outro endereço depois desta**. As demais
/// quando o conteúdo conferido não é um catálogo que este build lê.
pub fn ler_catalogo(
    bytes: &[u8],
    assinatura: &str,
    chave: &str,
) -> Result<Catalogo, FalhaNoCatalogo> {
    conferir_assinatura(bytes, assinatura, chave)?;
    let catalogo: Catalogo = serde_json::from_slice(bytes)
        .map_err(|erro| FalhaNoCatalogo::NaoEUmCatalogo(erro.to_string()))?;
    if catalogo.esquema > ESQUEMA_CONHECIDO {
        return Err(FalhaNoCatalogo::MaisNovoQueEsteBuild {
            pedido: catalogo.esquema,
            conhecido: ESQUEMA_CONHECIDO,
        });
    }
    Ok(catalogo)
}

/// O mesmo, para a lista de revogações.
///
/// # Errors
///
/// As mesmas de [`ler_catalogo`].
pub fn ler_revogacoes(
    bytes: &[u8],
    assinatura: &str,
    chave: &str,
) -> Result<Revogacoes, FalhaNoCatalogo> {
    conferir_assinatura(bytes, assinatura, chave)?;
    serde_json::from_slice(bytes).map_err(|erro| FalhaNoCatalogo::NaoEUmCatalogo(erro.to_string()))
}

/// A assinatura, contra a chave **embutida no build** que chama.
///
/// `chave` é o arquivo `.pub` **inteiro**, no formato cru do `minisign`, que é
/// o que o indexador comita; `assinatura`, o `.minisig` cru que ele publica ao
/// lado de cada arquivo. O mesmo `minisign-verify` do `seele-lancador`, com o
/// modo legado aceito: o indexador assina com `-l` (os bytes crus, e não o
/// BLAKE2b deles), e recusar o modo legado seria recusar tudo o que ele publica.
///
/// **A chave que não abre não é assinatura recusada**, e a diferença não é
/// sutil: a primeira é defeito do build e a segunda é um catálogo adulterado.
fn conferir_assinatura(bytes: &[u8], assinatura: &str, chave: &str) -> Result<(), FalhaNoCatalogo> {
    let chave = minisign_verify::PublicKey::decode(chave.trim()).map_err(|erro| {
        FalhaNoCatalogo::NaoEUmCatalogo(format!("a chave embutida neste SEELE não abre: {erro:?}"))
    })?;
    let assinatura = minisign_verify::Signature::decode(assinatura.trim())
        .map_err(|_| FalhaNoCatalogo::AssinaturaDoCatalogoRecusada)?;
    chave
        .verify(bytes, &assinatura, true)
        .map_err(|_| FalhaNoCatalogo::AssinaturaDoCatalogoRecusada)
}

/// Onde buscar cada arquivo de uma versão, e o hash que o conjunto tem de dar.
///
/// O caminho carrega a versão — `mods/<autor>/<nome>/<versao>/<arquivo>` — e é
/// isso que torna cada arquivo imutável: uma versão nova é um caminho novo, e
/// «o MOD que você baixou é o MOD que revisamos» vale sem mecanismo nenhum.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanoDeDownload {
    /// Cada par `(caminho dentro do MOD, URL absoluta)`.
    pub arquivos: Vec<(String, String)>,
    /// O hash que o conjunto baixado tem de dar.
    pub hash: String,
}

/// Monta o plano, ou diz por que não há.
///
/// # Errors
///
/// [`FalhaNoCatalogo::NaoEstaNoCatalogo`] quando o MOD ou a versão não existem,
/// e [`FalhaNoCatalogo::Revogada`] quando aquela versão foi retirada.
pub fn planejar(
    catalogo: &Catalogo,
    revogacoes: &Revogacoes,
    id: &str,
    versao: &str,
) -> Result<PlanoDeDownload, FalhaNoCatalogo> {
    let achado = catalogo
        .mods
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| FalhaNoCatalogo::NaoEstaNoCatalogo(id.to_owned()))?;
    let publicada = achado
        .versoes
        .iter()
        .find(|v| v.versao == versao)
        .ok_or_else(|| FalhaNoCatalogo::NaoEstaNoCatalogo(format!("{id} {versao}")))?;

    // **A revogação é conferida aqui, antes de baixar.** Baixar e recusar
    // depois gastaria a rede de quem usa para chegar à mesma resposta — e,
    // pior, deixaria os bytes de uma versão furada passarem pelo disco.
    if let Some(retirada) = revogacoes
        .mods
        .iter()
        .find(|r| r.id == id && r.versao == versao)
    {
        return Err(FalhaNoCatalogo::Revogada {
            motivo: retirada.motivo.clone(),
            corrigido_em: retirada.corrigido_em.clone(),
        });
    }

    let arquivos = publicada
        .arquivos
        .iter()
        .map(|relativo| {
            (
                relativo.clone(),
                format!("{ENDERECO}/mods/{id}/{versao}/{relativo}"),
            )
        })
        .collect();
    Ok(PlanoDeDownload {
        arquivos,
        hash: publicada.hash.clone(),
    })
}

/// Confere que o que chegou é o que o catálogo declarou.
///
/// # Errors
///
/// [`FalhaNoCatalogo::ConteudoDivergente`], com os dois números, para a tela
/// poder dizer qual é qual.
pub fn conferir_conjunto(
    baixados: &mut [(String, Vec<u8>)],
    esperado: &str,
) -> Result<(), FalhaNoCatalogo> {
    let obtido = crate::mods::hash_do_conjunto(baixados);
    if obtido.eq_ignore_ascii_case(esperado) {
        return Ok(());
    }
    Err(FalhaNoCatalogo::ConteudoDivergente {
        esperado: esperado.to_owned(),
        obtido,
    })
}

/// **O conjunto conferido serve a este SEELE?** As duas primeiras recusas do
/// `publicar_de` do desktop, na ordem dele, para quem guarda os bytes sem
/// pasta (o navegador): sem `mod.json`, e o manifesto que não vale. O `id` e o
/// hash do que vai ser guardado.
///
/// # Errors
///
/// [`FalhaAoInstalarMod::SemManifesto`] e
/// [`FalhaAoInstalarMod::ManifestoRecusado`].
pub fn pacote_que_serve(
    baixados: &mut [(String, Vec<u8>)],
) -> Result<(String, String), FalhaAoInstalarMod> {
    if !baixados.iter().any(|(caminho, _)| caminho == "mod.json") {
        return Err(FalhaAoInstalarMod::SemManifesto);
    }
    let (manifest, hash) = crate::mods::ler_pacote(baixados).map_err(|motivo| {
        FalhaAoInstalarMod::ManifestoRecusado(crate::mods::refusal_name(&motivo).to_owned())
    })?;
    Ok((manifest.id, crate::mods::hex(&hash)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chave_que_nao_abre_nao_e_assinatura_recusada() {
        assert!(matches!(
            ler_catalogo(b"{}", "x", "isto nao e uma chave"),
            Err(FalhaNoCatalogo::NaoEUmCatalogo(texto)) if texto.contains("não abre")
        ));
    }

    #[test]
    fn o_conjunto_baixado_sem_manifesto_ou_com_um_que_nao_vale_e_recusado_daqui() {
        let mut sem = vec![("cliente.js".to_owned(), Vec::new())];
        assert_eq!(
            pacote_que_serve(&mut sem),
            Err(FalhaAoInstalarMod::SemManifesto)
        );
        let mut velho = vec![(
            "mod.json".to_owned(),
            br#"{"schema":1,"id":"a/b","version":"1","api":2,"repo":"r","client":"c.js"}"#.to_vec(),
        )];
        assert_eq!(
            pacote_que_serve(&mut velho),
            Err(FalhaAoInstalarMod::ManifestoRecusado(
                "api-too-old".to_owned()
            ))
        );
    }
}
