//! O catálogo de MODs, do lado de quem baixa — ADR 0045.
//!
//! # O que faltava
//!
//! Tudo deste lado. O `SEELE-MODS-INDEXER` tem o gerador, o assinador e o site,
//! com 167 testes passando, e o cliente não tinha **uma linha** sobre o
//! catálogo: nenhuma referência a `catalogo.json`, e a chave pública de MOD não
//! estava embutida em lugar nenhum. O fluxo que `indexador-de-mods.md` descreve
//! — baixar, conferir a assinatura, buscar localmente — não existia.
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
//! # A parte pura e a parte de rede
//!
//! Tudo o que decide está em funções que recebem bytes: conferir a assinatura,
//! analisar, montar o plano de download, conferir o conjunto baixado. A rede é
//! uma casca fina por cima. É o que permite provar a recusa de um catálogo
//! adulterado sem levantar servidor nenhum — e é o mesmo corte que o
//! `seele-lancador` faz, pela mesma razão.

use serde::Deserialize;

/// A chave pública que este build confere, compilada para dentro dele.
///
/// Cópia byte a byte da que o indexador usa para assinar. Ver
/// `apps/seele-app/chaves/LEIA.md` para por que ela é a segunda chave e o que
/// custa trocá-la.
const CHAVE_DO_CATALOGO: &str = include_str!("../chaves/mods.pub");

/// Onde o catálogo mora.
///
/// Uma constante e não configuração: um endereço que a pessoa pudesse trocar
/// seria um endereço que alguém troca por ela. A assinatura protege o conteúdo;
/// o que protege a **origem** é não haver nada a apontar para outro lugar.
pub(crate) const ENDERECO: &str = "https://mods.seele.app.br";

/// Por que o catálogo não pôde ser usado.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) enum FalhaNoCatalogo {
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
}

/// O esquema de catálogo que este build lê.
const ESQUEMA_CONHECIDO: u32 = 1;

/// O catálogo inteiro.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub(crate) struct Catalogo {
    /// A versão do formato.
    pub(crate) esquema: u32,
    /// Quando foi gerado, em segundos desde a época.
    #[serde(default)]
    pub(crate) gerado_em: i64,
    /// Um por MOD.
    #[serde(default)]
    pub(crate) mods: Vec<ModNoCatalogo>,
}

/// Um MOD, como o catálogo o lista.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub(crate) struct ModNoCatalogo {
    /// `autor/nome`.
    pub(crate) id: String,
    /// O nome legível.
    #[serde(default)]
    pub(crate) titulo: String,
    /// Uma linha sobre o que ele faz.
    #[serde(default)]
    pub(crate) resumo: String,
    /// O repositório público.
    #[serde(default)]
    pub(crate) repo: String,
    /// O veredito mais recente, para listar e filtrar.
    ///
    /// **Não carimba a versão**: quem carimba um número de versão é o `nivel`
    /// de dentro de `versoes[]`, porque o veredito ao lado de um hash tem de
    /// ser o veredito daqueles bytes. O `indexador-de-mods.md` é explícito.
    #[serde(default)]
    pub(crate) nivel: String,
    /// As versões publicadas, da mais antiga para a mais nova.
    #[serde(default)]
    pub(crate) versoes: Vec<VersaoNoCatalogo>,
}

/// Uma versão publicada de um MOD.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub(crate) struct VersaoNoCatalogo {
    /// O número que o autor declara.
    pub(crate) versao: String,
    /// Contra que API do MOD ela foi escrita.
    #[serde(default)]
    pub(crate) api: u32,
    /// O hash do conjunto, em hexadecimal minúsculo.
    pub(crate) hash: String,
    /// O veredito **destes bytes**.
    #[serde(default)]
    pub(crate) nivel: String,
    /// Os avisos do terceiro nível, como identificadores.
    #[serde(default)]
    pub(crate) notas: Vec<String>,
    /// O que esta versão declara alcançar.
    #[serde(default)]
    pub(crate) alcanca: Vec<String>,
    /// A lista exata de arquivos a buscar.
    ///
    /// **O cliente não descobre nada.** Ele lê esta lista, busca aquilo, e
    /// confere o hash do conjunto — um catálogo adulterado que omitisse um
    /// arquivo mudaria o hash, e a conferência o pega.
    #[serde(default)]
    pub(crate) arquivos: Vec<String>,
}

/// Uma revogação: uma versão que saiu de circulação, e por quê.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub(crate) struct Revogacao {
    /// Qual MOD.
    pub(crate) id: String,
    /// Qual versão.
    pub(crate) versao: String,
    /// O identificador do defeito, da lista fechada.
    #[serde(default)]
    pub(crate) motivo: String,
    /// Em que versão ele foi corrigido, quando há uma.
    #[serde(default)]
    pub(crate) corrigido_em: Option<String>,
}

/// A lista de revogações inteira.
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub(crate) struct Revogacoes {
    /// A versão do formato.
    #[serde(default)]
    pub(crate) esquema: u32,
    /// As versões de MOD retiradas.
    #[serde(default)]
    pub(crate) mods: Vec<Revogacao>,
}

/// Confere a assinatura e só então analisa.
///
/// # Errors
///
/// [`FalhaNoCatalogo::AssinaturaDoCatalogoRecusada`] quando a assinatura não bate — e
/// **quem chama não tenta outro endereço depois desta**. As demais quando o
/// conteúdo conferido não é um catálogo que este build lê.
pub(crate) fn ler_catalogo(bytes: &[u8], assinatura: &str) -> Result<Catalogo, FalhaNoCatalogo> {
    conferir_assinatura(bytes, assinatura)?;
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
pub(crate) fn ler_revogacoes(
    bytes: &[u8],
    assinatura: &str,
) -> Result<Revogacoes, FalhaNoCatalogo> {
    conferir_assinatura(bytes, assinatura)?;
    serde_json::from_slice(bytes).map_err(|erro| FalhaNoCatalogo::NaoEUmCatalogo(erro.to_string()))
}

/// A assinatura, contra a chave **embutida neste build**.
///
/// Nunca contra uma chave que venha junto do arquivo: uma chave que chega pela
/// mesma porta que o conteúdo não prova nada sobre ele.
fn conferir_assinatura(bytes: &[u8], assinatura: &str) -> Result<(), FalhaNoCatalogo> {
    // O arquivo `.pub` **inteiro**, e não a segunda linha dele: `do_arquivo_pub`
    // lê o formato cru do `minisign`, que é o que o indexador comita. A outra
    // função, a do atualizador, espera base64 do arquivo — e passar uma para a
    // outra foi metade deste defeito.
    //
    // **A chave que não abre não é assinatura recusada**, e a diferença não é
    // sutil: a primeira é defeito deste build e a segunda é um catálogo
    // adulterado. Eu as juntei numa variante só, e o resultado foi o produto me
    // dizendo «a assinatura não confere» enquanto o erro era eu estar passando a
    // chave no formato errado — o produto sabendo e contando outra coisa.
    let chave = seele_lancador::Chave::do_arquivo_pub(CHAVE_DO_CATALOGO).map_err(|erro| {
        FalhaNoCatalogo::NaoEUmCatalogo(format!("a chave embutida neste SEELE não abre: {erro:?}"))
    })?;
    // `conferir_minisig` e não `conferir`: o indexador publica o `.minisig`
    // cru ao lado do arquivo, e não o base64 dele num campo de JSON. Ver o
    // porquê dos dois formatos na doc daquela função.
    chave
        .conferir_minisig(bytes, assinatura)
        .map_err(|_| FalhaNoCatalogo::AssinaturaDoCatalogoRecusada)
}

/// Onde buscar cada arquivo de uma versão, e o hash que o conjunto tem de dar.
///
/// O caminho carrega a versão — `mods/<autor>/<nome>/<versao>/<arquivo>` — e é
/// isso que torna cada arquivo imutável: uma versão nova é um caminho novo, e
/// «o MOD que você baixou é o MOD que revisamos» vale sem mecanismo nenhum.
#[derive(Debug, Clone)]
pub(crate) struct PlanoDeDownload {
    /// Cada par `(caminho dentro do MOD, URL absoluta)`.
    pub(crate) arquivos: Vec<(String, String)>,
    /// O hash que o conjunto baixado tem de dar.
    pub(crate) hash: String,
}

/// Monta o plano, ou diz por que não há.
///
/// # Errors
///
/// [`FalhaNoCatalogo::NaoEstaNoCatalogo`] quando o MOD ou a versão não existem,
/// e [`FalhaNoCatalogo::Revogada`] quando aquela versão foi retirada.
pub(crate) fn planejar(
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
pub(crate) fn conferir_conjunto(
    baixados: &mut [(String, Vec<u8>)],
    esperado: &str,
) -> Result<(), FalhaNoCatalogo> {
    let obtido = seele_ffi::mods::hash_do_conjunto(baixados);
    if obtido.eq_ignore_ascii_case(esperado) {
        return Ok(());
    }
    Err(FalhaNoCatalogo::ConteudoDivergente {
        esperado: esperado.to_owned(),
        obtido,
    })
}

/// Busca um arquivo e a assinatura dele, e devolve os dois.
///
/// Os dois na mesma função porque um sem o outro não serve para nada: um
/// conteúdo sem assinatura não pode ser lido, e uma assinatura sem conteúdo não
/// tem o que conferir. Separá-los deixaria um caminho em que o primeiro chega e
/// o segundo não, e alguém decide seguir.
async fn buscar_assinado(
    cliente: &reqwest::Client,
    caminho: &str,
) -> Result<(Vec<u8>, String), FalhaNoCatalogo> {
    let url = format!("{ENDERECO}/{caminho}");
    let bytes = cliente
        .get(&url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))?
        .bytes()
        .await
        .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))?;
    let assinatura = cliente
        .get(format!("{url}.minisig"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))?
        .text()
        .await
        .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))?;
    Ok((bytes.to_vec(), assinatura))
}

/// Um cliente HTTP com prazo. Sem prazo, uma rede que engole pendura a janela.
fn cliente() -> Result<reqwest::Client, FalhaNoCatalogo> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))
}

/// Baixa e confere o catálogo inteiro.
///
/// # Errors
///
/// [`FalhaNoCatalogo`], uma variante por motivo.
pub(crate) async fn buscar_catalogo() -> Result<Catalogo, FalhaNoCatalogo> {
    let cliente = cliente()?;
    let (bytes, assinatura) = buscar_assinado(&cliente, "catalogo.json").await?;
    ler_catalogo(&bytes, &assinatura)
}

/// Baixa e confere a lista de revogações.
///
/// **Nunca ao abrir o app** — ADR 0026. Ela é buscada ao hospedar e ao entrar,
/// que são os dois momentos em que ela decide alguma coisa; consultá-la no
/// arranque seria uma batida na rede por abrir o programa.
///
/// # Errors
///
/// [`FalhaNoCatalogo`], uma variante por motivo.
pub(crate) async fn buscar_revogacoes() -> Result<Revogacoes, FalhaNoCatalogo> {
    let cliente = cliente()?;
    let (bytes, assinatura) = buscar_assinado(&cliente, "revogacoes.json").await?;
    ler_revogacoes(&bytes, &assinatura)
}

/// Baixa uma versão do catálogo e a instala nesta máquina.
///
/// # A ordem, e ela é o contrato
///
/// Catálogo conferido, revogação conferida, plano montado, bytes baixados,
/// **hash do conjunto conferido**, e só então disco — pelo caminho de
/// instalação que já existe, que valida o manifesto e recusa sobrescrever um
/// MOD instalado. Nada é escrito antes de a conferência passar.
///
/// # Errors
///
/// [`FalhaNoCatalogo`] até a conferência; depois dela, o que a instalação
/// disser, traduzido.
pub(crate) async fn instalar_do_catalogo(
    config: &str,
    id: &str,
    versao: &str,
) -> Result<(), FalhaNoCatalogo> {
    let cliente = cliente()?;
    let catalogo = buscar_catalogo().await?;
    // A lista de revogações é buscada junto, e uma lista que não responde
    // **não** vira lista vazia: seguir sem ela é instalar sem saber se aquela
    // versão foi retirada, que é exatamente o caso em que ela importa.
    let revogacoes = buscar_revogacoes().await?;
    let plano = planejar(&catalogo, &revogacoes, id, versao)?;

    let mut baixados = Vec::with_capacity(plano.arquivos.len());
    for (relativo, url) in &plano.arquivos {
        let bytes = cliente
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))?
            .bytes()
            .await
            .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))?;
        baixados.push((relativo.clone(), bytes.to_vec()));
    }
    conferir_conjunto(&mut baixados, &plano.hash)?;

    // Escritos numa pasta de passagem e instalados pelo caminho de sempre: ele
    // valida o manifesto, recusa atalho e recusa sobrescrever. Um segundo
    // caminho de instalação seria um segundo conjunto de recusas para
    // discordar do primeiro.
    let passagem = std::env::temp_dir().join(format!(
        "seele-mod-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    for (relativo, bytes) in &baixados {
        let destino = passagem.join(relativo);
        if let Some(pai) = destino.parent() {
            std::fs::create_dir_all(pai)
                .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))?;
        }
        std::fs::write(&destino, bytes)
            .map_err(|erro| FalhaNoCatalogo::NaoRespondeu(erro.to_string()))?;
    }
    let resultado = crate::mods::instalar_de(std::path::Path::new(config), &passagem)
        .map(|_| ())
        .map_err(|erro| FalhaNoCatalogo::NaoEUmCatalogo(format!("{erro:?}")));
    let _ = std::fs::remove_dir_all(&passagem);
    resultado
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod o_catalogo {
    use super::{conferir_conjunto, ler_catalogo, planejar, Catalogo, FalhaNoCatalogo, Revogacoes};

    /// Um catálogo de mentira, no formato que o indexador publica.
    fn catalogo_json() -> Vec<u8> {
        serde_json::json!({
            "esquema": 1,
            "gerado_em": 1_757_100_000_i64,
            "mods": [{
                "id": "alguem/rpg",
                "titulo": "Salas de RPG",
                "resumo": "Fichas e dados.",
                "repo": "https://example.invalid/rpg",
                "nivel": "verificado",
                "versoes": [{
                    "versao": "1.0.0",
                    "api": 1,
                    "hash": "9f2c",
                    "nivel": "com-notas",
                    "notas": ["fala-com-terceiro"],
                    "alcanca": ["falar com um serviço de fora"],
                    "arquivos": ["mod.json", "cliente/main.js"]
                }]
            }]
        })
        .to_string()
        .into_bytes()
    }

    fn sem_revogacoes() -> Revogacoes {
        Revogacoes::default()
    }

    /// **O catálogo de verdade, assinado pelo indexador de verdade.**
    ///
    /// Os dois lados desta cadeia moram em repositórios diferentes, com suítes
    /// diferentes — e as duas podem ficar verdes enquanto discordam. Foi
    /// exatamente assim que este cliente passou meses sem uma linha sobre o
    /// catálogo enquanto o indexador tinha 167 testes passando.
    ///
    /// Este teste é a costura: ele lê o que o `ferramentas/gerar.py` produziu,
    /// com a assinatura que o `minisign` fez, contra a chave que este build
    /// carrega. Se qualquer um dos três mudar sem o outro saber, ele fica
    /// vermelho. Ver `apps/seele-app/testes/LEIA.md`.
    #[test]
    fn o_catalogo_que_o_indexador_gera_e_aceito_por_este_build() {
        let bytes = include_bytes!("../testes/catalogo-do-indexador.json");
        let assinatura = include_str!("../testes/catalogo-do-indexador.json.minisig");

        let catalogo = ler_catalogo(bytes, assinatura)
            .expect("o catálogo que o indexador assinou tem de passar neste cliente");
        assert_eq!(catalogo.esquema, 1);
        // O vetor era o catálogo vazio, e a afirmação era `mods.is_empty()`
        // com um recado pedindo que ela mudasse junto com o arquivo. Mudou: o
        // MESA é o primeiro MOD avaliado, e um catálogo com uma entrada de
        // verdade exercita o que o vazio não exercitava — a análise de um
        // `ModNoCatalogo` inteiro, com versão, hash, alcance e notas.
        let mesa = catalogo
            .mods
            .iter()
            .find(|m| m.id == "seele/mesa")
            .expect("o vetor do indexador tem de trazer o MESA");
        let versao = mesa
            .versoes
            .first()
            .expect("um MOD no catálogo sem versão nenhuma não é instalável");
        assert_eq!(versao.versao, "1.2.0");
        assert_eq!(versao.hash.len(), 64, "o hash do conteúdo é um SHA-256");
        // Que a API publicada caiba nesta build é conferido em
        // `seele-conformance`, e não aqui: esta casca não depende do
        // `seele-proto` (ADR 0039), e copiar `MOD_API_VERSION` para dentro dela
        // seria a terceira cópia do número cuja divergência custou a primeira
        // publicação.
        assert!(
            versao.api > 0,
            "uma versão sem API declarada não é instalável"
        );
        assert!(
            catalogo.gerado_em > 0,
            "o catálogo não diz quando foi gerado"
        );
    }

    #[test]
    fn as_revogacoes_que_o_indexador_gera_tambem_sao_aceitas() {
        let bytes = include_bytes!("../testes/revogacoes-do-indexador.json");
        let assinatura = include_str!("../testes/revogacoes-do-indexador.json.minisig");
        let revogacoes = super::ler_revogacoes(bytes, assinatura)
            .expect("a lista de revogações assinada tem de passar");
        assert!(revogacoes.mods.is_empty());
    }

    #[test]
    fn a_assinatura_do_indexador_nao_vale_para_outro_conteudo() {
        // A metade que prova que a conferência **confere**, e não que ela
        // aceita qualquer par. Sem ela, um `Ok` acima poderia vir de uma
        // verificação que sempre passa.
        let assinatura = include_str!("../testes/catalogo-do-indexador.json.minisig");
        assert!(matches!(
            ler_catalogo(br#"{"esquema":1,"mods":[]}"#, assinatura),
            Err(FalhaNoCatalogo::AssinaturaDoCatalogoRecusada)
        ));
    }

    #[test]
    fn um_catalogo_sem_assinatura_valida_nao_e_analisado() {
        // **A ordem é o teste.** Se a análise viesse antes, um JSON adulterado
        // decidiria o que este build faz antes de alguém provar que ele veio de
        // quem diz vir — que é escolher confiar nele para decidir se se confia
        // nele.
        assert!(matches!(
            ler_catalogo(&catalogo_json(), "isto-nao-e-uma-assinatura"),
            Err(FalhaNoCatalogo::AssinaturaDoCatalogoRecusada)
        ));
    }

    #[test]
    fn a_recusa_de_assinatura_nao_se_confunde_com_json_ruim() {
        // As duas respostas são opostas — «não tente de novo» e «o catálogo está
        // quebrado, avise quem publica» —, e uma só variante mandaria a pessoa
        // insistir contra um endereço sequestrado.
        assert!(matches!(
            ler_catalogo(b"{ isto nao e json }", "tambem-nao-e-assinatura"),
            Err(FalhaNoCatalogo::AssinaturaDoCatalogoRecusada),
        ));
    }

    /// Analisa direto, saltando a assinatura, para os testes que são sobre o
    /// **plano** e não sobre a procedência.
    fn catalogo_analisado() -> Catalogo {
        serde_json::from_slice(&catalogo_json()).unwrap()
    }

    #[test]
    fn o_plano_busca_exatamente_o_que_o_catalogo_lista() {
        let plano = planejar(
            &catalogo_analisado(),
            &sem_revogacoes(),
            "alguem/rpg",
            "1.0.0",
        )
        .expect("a versão está no catálogo");
        assert_eq!(
            plano.arquivos,
            vec![
                (
                    "mod.json".to_owned(),
                    "https://mods.seele.app.br/mods/alguem/rpg/1.0.0/mod.json".to_owned()
                ),
                (
                    "cliente/main.js".to_owned(),
                    "https://mods.seele.app.br/mods/alguem/rpg/1.0.0/cliente/main.js".to_owned()
                ),
            ],
            "o cliente tem de buscar a lista do catálogo, e não descobrir \
             arquivo por conta própria"
        );
        assert_eq!(plano.hash, "9f2c");
    }

    #[test]
    fn uma_versao_revogada_nao_chega_a_ser_baixada() {
        // Conferido **antes** do download: baixar e recusar depois gasta a rede
        // de quem usa para chegar à mesma resposta, e deixa os bytes de uma
        // versão furada passarem pelo disco.
        let revogacoes: Revogacoes = serde_json::from_value(serde_json::json!({
            "esquema": 1,
            "mods": [{
                "id": "alguem/rpg",
                "versao": "1.0.0",
                "motivo": "credencial-vazada",
                "corrigido_em": "1.0.1"
            }]
        }))
        .unwrap();

        let recusa = planejar(&catalogo_analisado(), &revogacoes, "alguem/rpg", "1.0.0");
        match recusa {
            Err(FalhaNoCatalogo::Revogada {
                motivo,
                corrigido_em,
            }) => {
                assert_eq!(motivo, "credencial-vazada");
                assert_eq!(
                    corrigido_em.as_deref(),
                    Some("1.0.1"),
                    "a recusa tem de dizer onde está o conserto, ou a pessoa \
                     fica sabendo que não pode e não o que fazer"
                );
            }
            outro => panic!("uma versão revogada foi planejada: {outro:?}"),
        }
    }

    #[test]
    fn o_que_nao_esta_no_catalogo_diz_isso_e_nao_estoura() {
        assert!(matches!(
            planejar(
                &catalogo_analisado(),
                &sem_revogacoes(),
                "ninguem/nada",
                "1.0.0"
            ),
            Err(FalhaNoCatalogo::NaoEstaNoCatalogo(_))
        ));
        assert!(matches!(
            planejar(
                &catalogo_analisado(),
                &sem_revogacoes(),
                "alguem/rpg",
                "9.9.9"
            ),
            Err(FalhaNoCatalogo::NaoEstaNoCatalogo(_))
        ));
    }

    #[test]
    fn um_arquivo_trocado_no_caminho_muda_o_hash_do_conjunto() {
        // É o que o desenho de arquivos soltos compra: o hash cobre o
        // **conjunto**, então um catálogo adulterado que trocasse ou omitisse
        // um arquivo não passa — sem segunda conferência e sem formato de
        // pacote.
        let mut honesto = vec![
            ("mod.json".to_owned(), b"{\"id\":\"a/b\"}".to_vec()),
            ("cliente/main.js".to_owned(), b"globalThis.x = 1;".to_vec()),
        ];
        let esperado = seele_ffi::mods::hash_do_conjunto(&mut honesto);
        assert!(conferir_conjunto(&mut honesto, &esperado).is_ok());

        let mut trocado = vec![
            ("mod.json".to_owned(), b"{\"id\":\"a/b\"}".to_vec()),
            (
                "cliente/main.js".to_owned(),
                b"globalThis.x = 1; roubar();".to_vec(),
            ),
        ];
        assert!(matches!(
            conferir_conjunto(&mut trocado, &esperado),
            Err(FalhaNoCatalogo::ConteudoDivergente { .. })
        ));

        // E omitir um arquivo também muda.
        let mut faltando = vec![("mod.json".to_owned(), b"{\"id\":\"a/b\"}".to_vec())];
        assert!(matches!(
            conferir_conjunto(&mut faltando, &esperado),
            Err(FalhaNoCatalogo::ConteudoDivergente { .. })
        ));
    }

    #[test]
    fn a_ordem_em_que_os_arquivos_chegam_nao_muda_o_hash() {
        // O download é concorrente e a ordem de chegada não é promessa de
        // ninguém. Se ela contasse, um MOD instalaria numa máquina e falharia
        // na outra, sem nada no código dizendo por quê.
        let mut numa_ordem = vec![
            ("mod.json".to_owned(), b"m".to_vec()),
            ("cliente/main.js".to_owned(), b"c".to_vec()),
            ("servidor/main.js".to_owned(), b"s".to_vec()),
        ];
        let mut noutra = vec![
            ("servidor/main.js".to_owned(), b"s".to_vec()),
            ("mod.json".to_owned(), b"m".to_vec()),
            ("cliente/main.js".to_owned(), b"c".to_vec()),
        ];
        assert_eq!(
            seele_ffi::mods::hash_do_conjunto(&mut numa_ordem),
            seele_ffi::mods::hash_do_conjunto(&mut noutra)
        );
    }
}
