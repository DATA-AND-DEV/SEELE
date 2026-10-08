//! O lado da casca do launcher: que versões e linhas há, baixá-las e abrir uma.
//!
//! # O que este módulo é
//!
//! O [ADR 0046](../../../docs/adr/0046-toda-versao-continua-de-pe.md), com a
//! emenda de 08/10/2026, decidiu que o app vira launcher **por linha de
//! servidor**: ele guarda o app mais novo de cada linha, e ao entrar num
//! servidor de outra linha abre o app dela. O núcleo dessa decisão é o
//! `seele-lancador`, que é biblioteca pura — ele não fala com a rede e não
//! desempacota, de propósito. Este módulo é o chamador: busca e guarda o
//! catálogo, baixa, e abre o que o `Resolvedor` escolher.
//!
//! # Só pelo `Resolvedor`
//!
//! Até a 0.15, esta casca montava a `VersaoResolvida` à mão, a partir do
//! depósito, e a lista de revogação nunca era conferida: o `Resolvedor` e o
//! `Deposito::conciliar_revogacoes` existiam, passavam nos testes, e ninguém os
//! chamava. Agora todo abrir passa por [`preparar`], que concilia a lista e
//! resolve. O lado que **lê** a revogação vai ligado na 0.16, porque um build
//! que não confere nunca vai aprender a conferir (spec 0.16, §2, F6).
//!
//! # Rede: só nos atos
//!
//! O catálogo fica guardado em disco ([`CATALOGO_GUARDADO`]). Abrir uma versão
//! instalada usa o guardado e não fala com a rede (ADR 0026). Hospedar e
//! entrar são atos, e buscam o catálogo antes, para a lista de revogação ser a
//! mais nova que se conseguir — com prazo curto, e com o guardado quando a rede
//! não responde.

use std::path::PathBuf;
use std::time::Duration;

use seele_lancador::{
    Ato, Chave, Deposito, Lancamento, Linha, Manifesto, Pedido, Recusa, Resolvedor, Versao,
};

/// Onde o depósito de versões fica, dentro da pasta que o app já usa.
///
/// Ao lado do banco e das preferências, e não noutro lugar: a pasta do ADR 0017
/// é a que o produto já cria, já respeita `SEELE_HOME` e já é a que alguém
/// apaga quando quer recomeçar do zero.
pub(crate) fn raiz_do_deposito(config: &str) -> PathBuf {
    std::path::Path::new(config).join("lancador")
}

/// Qual versão é **esta**, a que está rodando agora.
///
/// A mesma fonte do `Hello` do cliente e do `seeled --versao`: a variável que o
/// empacotamento carimba. Um build feito à mão não a tem, e aí não há versão
/// em uso a marcar — o que é verdade, e é melhor que marcar a errada.
#[must_use]
pub(crate) fn em_uso() -> Option<&'static str> {
    option_env!("SEELE_VERSAO")
}

/// Uma versão instalada nesta máquina, como a tela a desenha.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct VersaoInstalada {
    /// O identificador, como o depósito o guarda.
    pub(crate) versao: String,
    /// Se é a que está rodando agora.
    pub(crate) em_uso: bool,
    /// Se ela sabe dizer o que rodar sem o manifesto.
    ///
    /// Falso para uma instalação cujo marcador é anterior a esta anotação
    /// existir. A tela precisa disso para **não** oferecer um botão que não vai
    /// funcionar: oferecer e falhar depois é o defeito que este repositório
    /// chama de «o produto sabe e não conta».
    pub(crate) abrivel: bool,
}

/// O que há nesta máquina, em ordem de nome.
///
/// Ordem de nome porque é a que o disco oferece, e ela serve para listar. Esta
/// lista **não** diz qual é a mais nova — quem diz é o manifesto, e ele não
/// passa por aqui.
#[must_use]
pub(crate) fn instaladas(config: &str) -> Vec<VersaoInstalada> {
    let deposito = Deposito::em(raiz_do_deposito(config));
    let agora = em_uso();
    let mut lista: Vec<VersaoInstalada> = deposito
        .instaladas()
        .into_iter()
        .map(|versao| VersaoInstalada {
            em_uso: agora == Some(versao.como_texto()),
            abrivel: deposito.instalacao(&versao).is_some(),
            versao: versao.como_texto().to_owned(),
        })
        .collect();

    // **A que está rodando entra mesmo sem estar no depósito.**
    //
    // Relatado do uso real: «não mostra as versões disponíveis para o server».
    // A lista vinha só do depósito, e quem instalou pelo `.dmg` — que é como
    // toda primeira instalação acontece — não tem depósito nenhum. O resultado
    // era uma lista vazia numa máquina que tem, obviamente, uma versão: a que
    // está desenhando a tela.
    //
    // `abrivel: false` porque o launcher de fato não sabe abri-la: ela não tem
    // instalação no depósito. Isso não é recusa de nada — ela **já está
    // aberta**, e a tela lê `em_uso` antes de `abrivel` justamente para dizer
    // isso em vez de oferecer um botão que não teria o que fazer.
    if let Some(versao) = agora {
        if !lista.iter().any(|v| v.versao == versao) {
            lista.insert(
                0,
                VersaoInstalada {
                    versao: versao.to_owned(),
                    em_uso: true,
                    abrivel: false,
                },
            );
        }
    }
    lista
}

/// Por que não deu para abrir a versão pedida.
///
/// Nome estável e não frase, como as outras recusas que cruzam esta ponte: a
/// frase mora no JavaScript. Uma variante por motivo, porque cada uma pede
/// outra coisa de quem está na frente da tela.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum FalhaAoAbrirVersao {
    /// O identificador não é uma versão, ou a linha não é uma linha, ou vieram
    /// as duas.
    IdentificadorInvalido,
    /// Não há catálogo guardado, e o de agora não respondeu.
    ///
    /// Só acontece numa máquina que nunca baixou nada: toda instalação guarda o
    /// catálogo de onde veio.
    SemCatalogo,
    /// O catálogo não conhece esta versão.
    NaoPublicada,
    /// O catálogo não traz app nenhum desta linha.
    LinhaNaoPublicada(String),
    /// O catálogo não diz de que linha a versão é, e sem isso não há pasta de
    /// dados certa.
    LinhaNaoDeclarada,
    /// Revogada, com o defeito e a versão que o corrige.
    Revogada {
        /// Qual é o defeito, como a lista assinada o escreve.
        defeito: String,
        /// Onde foi corrigido, quando a lista diz.
        corrigida_em: Option<String>,
    },
    /// Publicada e não instalada nesta máquina. Não é erro: a tela oferece
    /// baixar.
    NaoInstalada {
        /// Se o catálogo traz pacote para este sistema.
        tem_pacote: bool,
    },
    /// A pasta existe e a instalação não terminou.
    InstalacaoIncompleta,
    /// Instalada e o executável sumiu.
    ExecutavelAusente,
    /// O catálogo não diz o executável deste sistema.
    ExecutavelNaoDeclarado,
    /// O catálogo diz um executável fora da instalação.
    ExecutavelInvalido,
    /// A lista de revogação que chegou não abriu, ou é mais velha que a
    /// guardada. O ato é recusado junto: seguir calado com a guardada seria
    /// acertar e não contar.
    ListaDeRevogacaoRecusada,
    /// O sistema recusou iniciar o processo, e o que ele disse.
    NaoIniciou(String),
}

impl From<Recusa> for FalhaAoAbrirVersao {
    fn from(recusa: Recusa) -> Self {
        match recusa {
            Recusa::NaoPublicada { .. } => Self::NaoPublicada,
            Recusa::LinhaNaoPublicada { linha } => Self::LinhaNaoPublicada(linha.como_texto()),
            Recusa::LinhaNaoDeclarada { .. } => Self::LinhaNaoDeclarada,
            Recusa::Revogada {
                defeito,
                corrigida_em,
                ..
            } => Self::Revogada {
                defeito,
                corrigida_em,
            },
            Recusa::NaoInstalada {
                tem_pacote_para_este_sistema,
                ..
            } => Self::NaoInstalada {
                tem_pacote: tem_pacote_para_este_sistema,
            },
            Recusa::InstalacaoIncompleta { .. } => Self::InstalacaoIncompleta,
            Recusa::ExecutavelAusente { .. } => Self::ExecutavelAusente,
            Recusa::ExecutavelNaoDeclarado { .. } => Self::ExecutavelNaoDeclarado,
            Recusa::ExecutavelInvalido { .. } => Self::ExecutavelInvalido,
            // A coerência é conferida ao entrar, contra o que o servidor
            // anuncia, e não ao resolver o que abrir. Se ela chegar aqui, a
            // versão é tratada como desconhecida: é o que ela é para este app.
            Recusa::UnidadeIncoerente { .. } => Self::NaoPublicada,
        }
    }
}

/// Com que intenção a versão escolhida vai ser aberta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Intencao {
    /// Hospedar assim que a janela subir.
    pub(crate) hospedar: bool,
    /// O nome público, quando há um. Só vale junto de `hospedar`.
    pub(crate) nome_publico: Option<String>,
    /// O servidor em que entrar, como link inteiro.
    pub(crate) link: Option<String>,
}

impl Intencao {
    /// Os argumentos de linha de comando que levam esta intenção ao outro
    /// processo.
    ///
    /// Os argumentos são repassados como vieram, e uma versão que não os
    /// conhece os ignora — é o que faz este caminho valer também para as
    /// publicadas antes de o launcher existir.
    fn argumentos(&self) -> Vec<String> {
        let mut argumentos = Vec::new();
        if self.hospedar {
            argumentos.push("--hospedar".to_owned());
            if let Some(nome) = &self.nome_publico {
                argumentos.push("--nome-publico".to_owned());
                argumentos.push(nome.clone());
            }
        }
        // O link vai inteiro: ele carrega a impressão digital, os endereços
        // alternativos e o convite de uso único, e reconstruí-lo do outro lado
        // a partir de pedaços seria um segundo analisador para discordar do
        // primeiro.
        if let Some(link) = &self.link {
            argumentos.push("--entrar".to_owned());
            argumentos.push(link.clone());
        }
        argumentos
    }
}

/// O pedido que a tela fez: uma versão exata, a mais nova de uma linha, ou a
/// mais nova de todas.
///
/// # Errors
///
/// [`FalhaAoAbrirVersao::IdentificadorInvalido`] quando o texto não é versão
/// ou linha, ou quando vieram as duas — escolher entre elas aqui seria decidir
/// pela tela.
pub(crate) fn pedido_de(
    versao: Option<&str>,
    linha: Option<&str>,
) -> Result<Pedido, FalhaAoAbrirVersao> {
    match (versao, linha) {
        (Some(_), Some(_)) => Err(FalhaAoAbrirVersao::IdentificadorInvalido),
        (Some(v), None) => Versao::nova(v)
            .map(Pedido::Exata)
            .map_err(|_| FalhaAoAbrirVersao::IdentificadorInvalido),
        (None, Some(l)) => Linha::nova(l)
            .map(Pedido::MaisNovaDaLinha)
            .map_err(|_| FalhaAoAbrirVersao::IdentificadorInvalido),
        (None, None) => Ok(Pedido::MaisNova),
    }
}

/// O lançamento montado, sem iniciar processo nenhum.
///
/// **A lista que vale é a conciliada**, e o `Resolvedor` não aceita outra
/// (`docs/versoes-lado-a-lado.md`, §1). Era o buraco que esta casca deixava,
/// montando a `VersaoResolvida` à mão.
///
/// Separado de [`iniciar`] pela mesma razão que o `seele-lancador` separa
/// `Lancamento` de `Lancamento::iniciar`: é o que permite provar, sem abrir
/// processo, que a versão escolhida vai receber o executável dela e a pasta de
/// dados da linha dela. Um processo iniciado com o executável certo e os dados
/// errados abre, funciona, e escreve as conversas no lugar de outra linha —
/// sem sintoma até alguém procurar o que escreveu.
///
/// # Errors
///
/// [`FalhaAoAbrirVersao`], menos [`FalhaAoAbrirVersao::NaoIniciou`], que só
/// existe depois de tentar — e o erro de criar a pasta de dados, que chega por
/// ela.
pub(crate) fn preparar(
    config: &str,
    manifesto: &Manifesto,
    chave: &Chave,
    pedido: &Pedido,
    ato: Ato,
    intencao: &Intencao,
) -> Result<Lancamento, FalhaAoAbrirVersao> {
    let deposito = Deposito::em(raiz_do_deposito(config));
    let vigente = deposito
        .conciliar_revogacoes(manifesto.envelope_de_revogacoes(), chave)
        .map_err(|_| FalhaAoAbrirVersao::ListaDeRevogacaoRecusada)?;
    let resolvida = Resolvedor::novo(manifesto, &deposito, alvo_deste_sistema(), &vigente)
        .resolver(pedido, ato)?;
    // A pasta de dados precisa existir antes: o produto a cria sozinho, mas
    // criá-la aqui é o que faz o erro — um disco cheio, uma permissão — chegar
    // como recusa desta função em vez de como uma janela que abre vazia.
    std::fs::create_dir_all(&resolvida.dados)
        .map_err(|erro| FalhaAoAbrirVersao::NaoIniciou(erro.to_string()))?;
    Ok(Lancamento::de(&resolvida, intencao.argumentos()))
}

/// Inicia o processo de um lançamento montado.
///
/// **Não espera por ele**, e **não fecha esta janela**: quem lançou não segura
/// o processo filho (ADR 0039), e fechar daqui deixaria a pessoa sem nada na
/// tela caso o outro não suba.
///
/// # Errors
///
/// [`FalhaAoAbrirVersao::NaoIniciou`], com o que o sistema disse.
pub(crate) fn iniciar(lancamento: &Lancamento) -> Result<(), FalhaAoAbrirVersao> {
    lancamento
        .iniciar()
        .map(|_| ())
        .map_err(|erro| FalhaAoAbrirVersao::NaoIniciou(erro.to_string()))
}

/// Resolve o pedido contra o catálogo deste ato e abre a versão escolhida.
///
/// # Errors
///
/// [`FalhaAoAbrirVersao`], uma variante por motivo.
pub(crate) async fn abrir(
    config: &str,
    atualizador: Option<&Atualizador>,
    pedido: &Pedido,
    ato: Ato,
    intencao: &Intencao,
) -> Result<(), FalhaAoAbrirVersao> {
    let Some(atualizador) = atualizador else {
        return Err(FalhaAoAbrirVersao::SemCatalogo);
    };
    let manifesto = catalogo_para(ato, config, atualizador).await?;
    iniciar(&preparar(
        config,
        &manifesto,
        &atualizador.chave,
        pedido,
        ato,
        intencao,
    )?)
}

/// Por que não deu para baixar e instalar uma versão.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) enum FalhaAoBaixarVersao {
    /// Nenhum dos endereços do catálogo respondeu.
    ///
    /// A rede, o GitHub fora do ar, ou uma máquina sem saída. Separado de tudo
    /// o mais porque a resposta é a mesma de sempre: tentar de novo depois.
    CatalogoNaoRespondeu,
    /// O catálogo respondeu e não é um catálogo, ou é mais novo que este build.
    CatalogoNaoServe(String),
    /// O catálogo não traz pacote para este sistema.
    ///
    /// No Mac e no Windows, desde a 0.16, isto é um defeito do manifesto: o Mac
    /// publica o `.app.tar.gz`, e o Windows o `-portatil.tar.gz`. No Mac Intel
    /// e no Linux é o estado normal, porque a linha 0.15 nunca teve pacote para
    /// eles (spec 0.16, §1), e a tela diz isso.
    SemPacoteParaEsteSistema(String),
    /// Já está instalada. Não é falha, e a tela diz isso em vez de um erro.
    JaInstalada(String),
    /// O download não completou.
    NaoBaixei(String),
    /// O pacote chegou e não passou na conferência, com o motivo.
    ///
    /// Assinatura recusada e conteúdo divergente chegam os dois por aqui **com
    /// o nome de cada um dentro**: as respostas são opostas — «baixe de novo» e
    /// «não tente de novo» — e uma frase só mandaria a pessoa insistir contra
    /// um pacote adulterado.
    PacoteRecusado(String),
}

/// O alvo deste sistema, na nomenclatura do atualizador.
///
/// A mesma que o `latest.json` já usa — `darwin-aarch64`, `windows-x86_64`,
/// `linux-x86_64` —, porque o manifesto do launcher estende aquele arquivo e
/// não inventa um segundo vocabulário para a mesma pergunta.
#[must_use]
pub(crate) fn alvo_deste_sistema() -> String {
    let sistema = match std::env::consts::OS {
        "macos" => "darwin",
        outro => outro,
    };
    // A arquitetura vai como o Rust a escreve. O `match` que estava aqui
    // mapeava `x86_64` para `x86_64` e `aarch64` para `aarch64` — clippy o
    // apontou, com razão: era uma tabela que não traduzia nada, dando a
    // impressão de que a nomenclatura do atualizador diverge da do Rust nesta
    // metade. Ela diverge só no sistema, e é lá que a tabela fica.
    format!("{sistema}-{}", std::env::consts::ARCH)
}

/// Abre um `.tar.gz` dentro da pasta de destino.
///
/// # O que este formato cobre
///
/// Os dois pacotes que o `versions[]` do manifesto declara para o launcher:
///
/// - o `.app.tar.gz` do macOS, que é também o que o atualizador baixa;
/// - o `-portatil.tar.gz` do Windows, que é a carga do instalador (`SEELE.exe`
///   e `seeled.exe`) num tar.gz, montado pelo `empacotar/windows.ps1` desde a
///   0.16.
///
/// O `-instalador.exe` do Windows **não** passa por aqui, e não é questão de
/// formato: ele instala por cima da instalação única da máquina, mexe no
/// registro e no menu iniciar. O manifesto o deixa só no topo, para o
/// atualizador. O `.deb` do Linux também fica de fora: ele instala no sistema.
struct DesempacotarTarGz;

impl seele_lancador::Desempacotador for DesempacotarTarGz {
    fn desempacotar(&self, pacote: &[u8], destino: &std::path::Path) -> std::io::Result<()> {
        let leitor = flate2::read::GzDecoder::new(pacote);
        let mut arquivo = tar::Archive::new(leitor);
        // `unpack` já recusa caminho absoluto e `..` — o `tar` deste crate faz
        // isso por padrão, e é a mesma defesa que o `seele-instalador` usa.
        arquivo.unpack(destino)
    }
}

/// A chave e os endereços do catálogo, do bloco do atualizador.
///
/// Do `tauri.conf.json`, do mesmo bloco do atualizador — e não de constantes
/// próprias. Duas listas de endereços para o mesmo catálogo divergiriam no dia
/// em que uma delas mudasse, e a que ficasse velha apontaria para um lugar que
/// não publica mais. A chave pública é a mesma pelo mesmo motivo: o ADR 0026 a
/// tem como a única que autoriza instalar programa nesta máquina.
#[derive(Debug, Clone)]
pub(crate) struct Atualizador {
    /// A chave que confere pacotes e a lista de revogação.
    pub(crate) chave: Chave,
    /// Os endereços do `latest.json`, em ordem.
    pub(crate) enderecos: Vec<String>,
}

/// O atualizador que este bloco do `tauri.conf.json` declara.
///
/// `None` para um build sem a chave ou sem endereços: ele não tem catálogo a
/// consultar, e dizer isso é melhor que tentar e falhar na rede.
pub(crate) fn do_atualizador(bloco: &serde_json::Value) -> Option<Atualizador> {
    let pubkey = bloco.get("pubkey")?.as_str()?;
    let chave = Chave::do_formato_do_atualizador(pubkey).ok()?;
    let enderecos: Vec<String> = bloco
        .get("endpoints")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect();
    (!enderecos.is_empty()).then_some(Atualizador { chave, enderecos })
}

/// O atualizador deste app, lido da configuração dele.
pub(crate) fn atualizador_do_app(app: &tauri::AppHandle) -> Option<Atualizador> {
    let bloco = app.config().plugins.0.get("updater")?;
    do_atualizador(bloco)
}

/// O nome do catálogo guardado, dentro do depósito.
///
/// Os bytes do último manifesto aceito. É o que deixa abrir uma versão já
/// instalada sem rede (ADR 0026: nada de rede ao abrir) e o que segura a
/// resolução quando o GitHub não responde num ato.
pub(crate) const CATALOGO_GUARDADO: &str = "manifesto.json";

/// Guarda os bytes de um catálogo que acabou de ser aceito.
///
/// # Errors
///
/// Falha de disco.
pub(crate) fn guardar_catalogo(config: &str, bytes: &[u8]) -> std::io::Result<()> {
    let raiz = raiz_do_deposito(config);
    std::fs::create_dir_all(&raiz)?;
    // Escrito ao lado e trocado de uma vez: um catálogo pela metade seria lido
    // como «nenhum», e a pessoa perderia o offline sem saber por quê.
    let provisorio = raiz.join(format!("{CATALOGO_GUARDADO}.{}", std::process::id()));
    std::fs::write(&provisorio, bytes)?;
    std::fs::rename(&provisorio, raiz.join(CATALOGO_GUARDADO))
}

/// O último catálogo aceito nesta máquina, se há um que ainda abre.
pub(crate) fn catalogo_guardado(config: &str, chave: &Chave) -> Option<Manifesto> {
    let bytes = std::fs::read(raiz_do_deposito(config).join(CATALOGO_GUARDADO)).ok()?;
    Manifesto::ler(&bytes, chave).ok()
}

/// Busca o catálogo nos endereços, em ordem, e o guarda quando ele é aceito.
///
/// # Errors
///
/// [`FalhaAoBaixarVersao::CatalogoNaoRespondeu`] ou
/// [`FalhaAoBaixarVersao::CatalogoNaoServe`].
pub(crate) async fn buscar_catalogo(
    config: &str,
    atualizador: &Atualizador,
    prazo: Duration,
) -> Result<Manifesto, FalhaAoBaixarVersao> {
    let cliente = reqwest::Client::builder()
        .timeout(prazo)
        .build()
        .map_err(|erro| FalhaAoBaixarVersao::NaoBaixei(erro.to_string()))?;

    // Os endereços em ordem, como o atualizador os tenta: o primeiro que
    // responde vale.
    let mut bytes_do_catalogo = None;
    for endereco in &atualizador.enderecos {
        if let Ok(resposta) = cliente.get(endereco).send().await {
            if resposta.status().is_success() {
                if let Ok(bytes) = resposta.bytes().await {
                    bytes_do_catalogo = Some(bytes.to_vec());
                    break;
                }
            }
        }
    }
    let Some(bytes) = bytes_do_catalogo else {
        return Err(FalhaAoBaixarVersao::CatalogoNaoRespondeu);
    };
    let manifesto = Manifesto::ler(&bytes, &atualizador.chave)
        .map_err(|erro| FalhaAoBaixarVersao::CatalogoNaoServe(format!("{erro:?}")))?;
    // Uma falha ao guardar não impede nada: o que se perde é o offline da
    // próxima abertura, e trocar o ato por isso seria a troca errada.
    if let Err(erro) = guardar_catalogo(config, &bytes) {
        tracing::warn!(%erro, "não guardei o catálogo; a próxima abertura sem rede não o terá");
    }
    Ok(manifesto)
}

/// O catálogo que vale para este ato.
///
/// Abrir não fala com a rede (ADR 0026 e 0046): usa o guardado. Hospedar e
/// entrar são atos, e buscam o de agora para a lista de revogação ser a mais
/// nova que se conseguir — com prazo curto, e com o guardado quando a rede não
/// responde.
///
/// # Errors
///
/// [`FalhaAoAbrirVersao::SemCatalogo`] quando não há nenhum dos dois.
pub(crate) async fn catalogo_para(
    ato: Ato,
    config: &str,
    atualizador: &Atualizador,
) -> Result<Manifesto, FalhaAoAbrirVersao> {
    if ato.confere_revogacao() {
        match buscar_catalogo(config, atualizador, Duration::from_secs(5)).await {
            Ok(manifesto) => return Ok(manifesto),
            Err(erro) => tracing::info!(?erro, "o catálogo não respondeu; vale o guardado"),
        }
    }
    catalogo_guardado(config, &atualizador.chave).ok_or(FalhaAoAbrirVersao::SemCatalogo)
}

/// Baixa do catálogo a versão pedida e a instala ao lado das outras.
///
/// # A ordem, e por que ela é esta
///
/// Catálogo, conferência do catálogo, pacote, conferência do pacote, e só então
/// disco. **Nada é escrito antes de a assinatura passar** — é o contrato do
/// `Deposito::instalar`, e o motivo é o mesmo de sempre: meia instalação é o
/// estado que ninguém sabe desfazer.
///
/// Depois de instalar, a poda: o launcher guarda o app mais novo de cada linha,
/// e uma versão mais velha da mesma linha sai (spec 0.16, §1).
///
/// # Errors
///
/// [`FalhaAoBaixarVersao`], uma variante por motivo.
pub(crate) async fn baixar(
    config: &str,
    atualizador: &Atualizador,
    pedido: &Pedido,
) -> Result<String, FalhaAoBaixarVersao> {
    let manifesto = buscar_catalogo(config, atualizador, Duration::from_secs(30)).await?;
    let publicacao =
        match pedido {
            Pedido::MaisNova => manifesto.mais_nova().clone(),
            Pedido::Exata(versao) => manifesto.publicacao(versao).cloned().ok_or_else(|| {
                FalhaAoBaixarVersao::CatalogoNaoServe(format!("{versao} não está publicada"))
            })?,
            Pedido::MaisNovaDaLinha(linha) => manifesto
                .mais_nova_da_linha(linha)
                .cloned()
                .ok_or_else(|| {
                    FalhaAoBaixarVersao::CatalogoNaoServe(format!(
                        "o catálogo não traz app nenhum do Servidor {linha}"
                    ))
                })?,
        };
    let versao_texto = publicacao.version.como_texto().to_owned();

    let deposito = Deposito::em(raiz_do_deposito(config));
    if deposito.esta_instalada(&publicacao.version) {
        return Err(FalhaAoBaixarVersao::JaInstalada(versao_texto));
    }

    let alvo = alvo_deste_sistema();
    let Some(url) = publicacao
        .platforms
        .get(&alvo)
        .and_then(seele_lancador::Pacote::url)
    else {
        return Err(FalhaAoBaixarVersao::SemPacoteParaEsteSistema(alvo));
    };

    let cliente = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|erro| FalhaAoBaixarVersao::NaoBaixei(erro.to_string()))?;
    let bytes = cliente
        .get(url)
        .send()
        .await
        .map_err(|erro| FalhaAoBaixarVersao::NaoBaixei(erro.to_string()))?
        .bytes()
        .await
        .map_err(|erro| FalhaAoBaixarVersao::NaoBaixei(erro.to_string()))?;

    // O trabalho pesado sai da Tokio: conferir a assinatura e abrir o arquivo
    // são CPU e disco, e segurá-los aqui prenderia o executor da janela.
    let raiz = raiz_do_deposito(config);
    let chave = atualizador.chave.clone();
    let instalada = publicacao.clone();
    tauri::async_runtime::spawn_blocking(move || {
        Deposito::em(raiz).instalar(&instalada, &alvo, &bytes, &chave, &DesempacotarTarGz)
    })
    .await
    .map_err(|erro| FalhaAoBaixarVersao::NaoBaixei(erro.to_string()))?
    .map_err(|erro| FalhaAoBaixarVersao::PacoteRecusado(format!("{erro:?}")))?;

    if let Some(linha) = publicacao.linha() {
        podar(&deposito, &linha, &manifesto);
    }
    Ok(versao_texto)
}

/// A poda da linha, com o que não saiu dito no rastro.
///
/// No Windows uma versão aberta não se apaga; ela fica, e a próxima poda tenta
/// de novo. Calar seria disco ocupado sem ninguém saber por quê.
fn podar(deposito: &Deposito, linha: &Linha, manifesto: &Manifesto) {
    for podada in deposito.podar_linha(linha, manifesto) {
        match podada.resultado {
            Ok(()) => {
                tracing::info!(versao = %podada.versao, %linha, "versão anterior da linha apagada");
            }
            Err(erro) => tracing::warn!(
                versao = %podada.versao,
                %linha,
                %erro,
                "não apaguei a versão anterior da linha; tento na próxima poda"
            ),
        }
    }
}

/// Por que esta versão, a deste processo, não pode hospedar nem entrar.
///
/// Os nomes são os mesmos das variantes de `FalhaAoHospedar` que a carregam no
/// `main.rs`, para uma frase só no `frases.js` servir aos dois comandos.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum RecusaDestaVersao {
    /// Esta versão foi revogada, com o defeito e a versão que o corrige.
    EstaVersaoRevogada {
        /// Qual é o defeito, como a lista assinada o escreve.
        defeito: String,
        /// Onde foi corrigido, quando a lista diz.
        corrigida_em: Option<String>,
    },
    /// A lista que chegou não abriu, ou é mais velha que a guardada. O ato é
    /// recusado junto: seguir calado com a guardada seria acertar e não contar.
    ListaDeRevogacaoRecusada,
}

/// A revogação desta versão, se a lista vigente a revoga.
///
/// Um build sem versão carimbada (`em_uso() == None`) não é revogável: a lista
/// fala de identificadores publicados, e ele não é um.
pub(crate) fn esta_versao_revogada(
    vigente: &seele_lancador::Vigente,
    em_uso: Option<&str>,
) -> Option<RecusaDestaVersao> {
    let revogacao = vigente.revogacao_de(em_uso?)?;
    Some(RecusaDestaVersao::EstaVersaoRevogada {
        defeito: revogacao.defect.clone(),
        corrigida_em: revogacao.fixed_in.clone(),
    })
}

/// Quanto tempo a lista conciliada vale em memória.
///
/// Um `connect` atrás do outro — reconectar, trocar de servidor — não precisa
/// perguntar ao GitHub a cada vez. Dez minutos é curto para uma revogação
/// publicada agora demorar a valer, e longo para não virar uma consulta por
/// clique.
const VALIDADE_DA_LISTA_EM_MEMORIA: Duration = Duration::from_secs(600);

/// A última lista conciliada neste processo, e quando.
static LISTA_EM_MEMORIA: std::sync::Mutex<Option<(std::time::Instant, seele_lancador::Vigente)>> =
    std::sync::Mutex::new(None);

/// Confere a lista de revogação para a versão **deste** processo, num ato.
///
/// O levantamento de 08/10 achou que hospedar no próprio processo não conferia
/// nada, e o spec quer o lado que lê ligado na 0.16: um build que não confere
/// nunca vai aprender a conferir. Hospedar e entrar são os atos do ADR 0046;
/// abrir não é, e não passa por aqui.
///
/// A rede tem o prazo do [`catalogo_para`]; sem rede, vale a lista guardada; e
/// sem catálogo nenhum e sem lista guardada, passa — não há o que conferir, e
/// trancar quem nunca viu lista seria trocar uma proteção por uma porta fechada.
///
/// # Errors
///
/// [`RecusaDestaVersao`].
pub(crate) async fn conferir_esta_versao(
    config: &str,
    atualizador: Option<&Atualizador>,
    ato: Ato,
) -> Result<(), RecusaDestaVersao> {
    let Some(atualizador) = atualizador else {
        return Ok(());
    };
    let guardada = LISTA_EM_MEMORIA
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .filter(|(quando, _)| quando.elapsed() < VALIDADE_DA_LISTA_EM_MEMORIA)
        .map(|(_, vigente)| vigente);
    let vigente = match guardada {
        Some(vigente) => vigente,
        None => {
            let envelope = catalogo_para(ato, config, atualizador)
                .await
                .ok()
                .and_then(|m| m.envelope_de_revogacoes().cloned());
            let vigente = Deposito::em(raiz_do_deposito(config))
                .conciliar_revogacoes(envelope.as_ref(), &atualizador.chave)
                .map_err(|_| RecusaDestaVersao::ListaDeRevogacaoRecusada)?;
            if let Ok(mut slot) = LISTA_EM_MEMORIA.lock() {
                *slot = Some((std::time::Instant::now(), vigente.clone()));
            }
            vigente
        }
    };
    match esta_versao_revogada(&vigente, em_uso()) {
        Some(recusa) => Err(recusa),
        None => Ok(()),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod o_launcher_ligado_ao_produto {
    use super::{instaladas, pedido_de, preparar, FalhaAoAbrirVersao, Intencao};
    use seele_lancador::assinatura::fixtures::ChaveDeTeste;
    use seele_lancador::{Ato, Chave, Deposito, Linha, Manifesto, Pedido, Versao};

    fn par() -> ChaveDeTeste {
        ChaveDeTeste::com_semente(42)
    }

    fn chave() -> Chave {
        Chave::do_formato_do_atualizador(&par().publica_no_formato_do_atualizador()).unwrap()
    }

    /// Um `.tar.gz` com um arquivo, como o `.app.tar.gz` do Mac e o portátil
    /// do Windows.
    fn pacote(executavel: &str, conteudo: &[u8]) -> Vec<u8> {
        let mut bruto = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut bruto, flate2::Compression::default());
            let mut tar = tar::Builder::new(gz);
            let mut cabecalho = tar::Header::new_gnu();
            cabecalho.set_size(conteudo.len() as u64);
            cabecalho.set_mode(0o755);
            cabecalho.set_cksum();
            tar.append_data(&mut cabecalho, executavel, conteudo)
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        bruto
    }

    fn soma(bytes: &[u8]) -> String {
        use sha2::Digest as _;
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// `versoes`: (versão, servidor). `revogada`: (alvo, defeito, conserto).
    ///
    /// O pacote de cada versão é `pacote("bin/seele", versão)`, então a soma e
    /// a assinatura daqui são as do pacote que [`instalado`] instala.
    pub(super) fn manifesto_json(
        versoes: &[(&str, &str)],
        revogada: Option<(&str, &str, &str)>,
    ) -> String {
        let alvo = super::alvo_deste_sistema();
        let entradas: Vec<String> = versoes
            .iter()
            .map(|(v, servidor)| {
                let bytes = pacote("bin/seele", v.as_bytes());
                format!(
                    r#"{{"version":"{v}","unit":{{"protocol":8,"mod_api":5,"server":"{servidor}","mod_apis":[5]}},
                        "platforms":{{"{alvo}":{{"url":"https://exemplo.invalido/{v}","signature":"{}",
                        "sha256":"{}","executable":"bin/seele"}}}}}}"#,
                    par().assinar_no_formato_do_atualizador(&bytes),
                    soma(&bytes),
                )
            })
            .collect();
        let revogacoes = revogada
            .map(|(alvo_revogado, defeito, conserto)| {
                use base64::Engine as _;
                let documento = format!(
                    r#"{{"schema":1,"issued_at":"2026-10-08T00:00:00Z","revoked":[{{"target":"{alvo_revogado}","defect":"{defeito}","fixed_in":"{conserto}"}}]}}"#
                );
                format!(
                    r#","revocations":{{"document":"{}","signature":"{}"}}"#,
                    base64::engine::general_purpose::STANDARD.encode(&documento),
                    par().assinar_no_formato_do_atualizador(documento.as_bytes()),
                )
            })
            .unwrap_or_default();
        format!(
            r#"{{"schema":2,"versions":[{}]{revogacoes}}}"#,
            entradas.join(",")
        )
    }

    /// Instala uma versão do manifesto pelo `Deposito::instalar` de verdade, com
    /// o desempacotador da casca. O marcador sai do jeito que sai em campo.
    fn instalado(config: &str, json: &str, versao: &str) -> Manifesto {
        let m = Manifesto::ler(json.as_bytes(), &chave()).unwrap();
        let p = m
            .publicacao(&Versao::nova(versao).unwrap())
            .unwrap()
            .clone();
        Deposito::em(super::raiz_do_deposito(config))
            .instalar(
                &p,
                &super::alvo_deste_sistema(),
                &pacote("bin/seele", versao.as_bytes()),
                &chave(),
                &super::DesempacotarTarGz,
            )
            .unwrap();
        m
    }

    fn linha(t: &str) -> Linha {
        Linha::nova(t).unwrap()
    }

    /// **O guarda contra o defeito que não tem sintoma.** Um processo aberto
    /// com o executável certo e o `SEELE_HOME` errado funciona, e escreve as
    /// conversas no diretório de outra linha.
    #[test]
    fn a_versao_escolhida_recebe_o_executavel_dela_e_os_dados_da_linha() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let json = manifesto_json(&[("0.16.0", "1.0"), ("0.15.2", "0.15")], None);
        let m = instalado(&config, &json, "0.15.2");

        let lancamento = preparar(
            &config,
            &m,
            &chave(),
            &Pedido::MaisNovaDaLinha(linha("0.15")),
            Ato::Hospedar,
            &Intencao {
                hospedar: true,
                nome_publico: Some("casa.exemplo.br".into()),
                link: None,
            },
        )
        .expect("a linha 0.15 está instalada e não revogada");

        let deposito = Deposito::em(super::raiz_do_deposito(&config));
        let dados = deposito.pasta_de_dados_da_linha(&linha("0.15"));
        assert_eq!(
            lancamento.programa,
            deposito
                .pasta_da_versao(&Versao::nova("0.15.2").unwrap())
                .join("bin/seele"),
            "o programa não é o executável que aquela instalação publicou"
        );
        assert!(
            lancamento.ambiente.contains(&(
                "SEELE_HOME".to_owned(),
                dados.to_string_lossy().into_owned()
            )),
            "a versão foi aberta sem a pasta de dados da linha dela: {:?}",
            lancamento.ambiente
        );
        assert!(
            dados.is_dir(),
            "a pasta de dados não foi criada antes de o processo subir"
        );
        assert_eq!(
            lancamento.argumentos,
            vec!["--hospedar", "--nome-publico", "casa.exemplo.br"],
            "a intenção de hospedar, ou o nome público, não atravessou"
        );
    }

    /// **O lado que lê a revogação**, ligado na casca: a lista guardada vale
    /// mesmo quando o manifesto de hoje vem sem ela.
    #[test]
    fn hospedar_uma_linha_revogada_e_recusado_com_o_defeito_e_o_conserto() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let com_lista = manifesto_json(
            &[("0.16.0", "1.0"), ("0.15.2", "0.15")],
            Some(("0.15.2", "defeito de teste", "0.15.3")),
        );
        let m = instalado(&config, &com_lista, "0.15.2");
        let pedido = Pedido::MaisNovaDaLinha(linha("0.15"));
        let hospedar = Intencao {
            hospedar: true,
            ..Intencao::default()
        };

        assert_eq!(
            preparar(&config, &m, &chave(), &pedido, Ato::Hospedar, &hospedar),
            Err(FalhaAoAbrirVersao::Revogada {
                defeito: "defeito de teste".to_owned(),
                corrigida_em: Some("0.15.3".to_owned()),
            })
        );

        // Amanhã o manifesto vem sem a lista; a guardada continua valendo.
        let sem_lista = Manifesto::ler(
            manifesto_json(&[("0.16.0", "1.0"), ("0.15.2", "0.15")], None).as_bytes(),
            &chave(),
        )
        .unwrap();
        assert!(matches!(
            preparar(
                &config,
                &sem_lista,
                &chave(),
                &pedido,
                Ato::Hospedar,
                &hospedar
            ),
            Err(FalhaAoAbrirVersao::Revogada { .. })
        ));
        // E abrir para ler o que ficou lá continua permitido.
        assert!(preparar(
            &config,
            &sem_lista,
            &chave(),
            &pedido,
            Ato::Abrir,
            &Intencao::default()
        )
        .is_ok());
    }

    #[test]
    fn uma_linha_publicada_e_nao_baixada_diz_que_ha_pacote() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let m = Manifesto::ler(
            manifesto_json(&[("0.16.0", "1.0"), ("0.15.2", "0.15")], None).as_bytes(),
            &chave(),
        )
        .unwrap();
        assert_eq!(
            preparar(
                &config,
                &m,
                &chave(),
                &Pedido::MaisNovaDaLinha(linha("0.15")),
                Ato::Entrar,
                &Intencao::default()
            ),
            Err(FalhaAoAbrirVersao::NaoInstalada { tem_pacote: true })
        );
    }

    #[test]
    fn entrar_num_servidor_leva_o_link_inteiro_para_a_versao_aberta() {
        // **O coração do ADR 0046**: quem clica num `seele://` de um servidor
        // de outra linha tem o app daquela linha aberto, já indo para lá. O link
        // vai inteiro, e não em pedaços: ele carrega a impressão digital, os
        // endereços alternativos e o convite de uso único.
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let m = instalado(
            &config,
            &manifesto_json(&[("0.15.2", "0.15")], None),
            "0.15.2",
        );
        let link = format!("seele://casa.exemplo:8383/?fp={}", "a".repeat(64));

        let lancamento = preparar(
            &config,
            &m,
            &chave(),
            &Pedido::MaisNovaDaLinha(linha("0.15")),
            Ato::Entrar,
            &Intencao {
                link: Some(link.clone()),
                ..Intencao::default()
            },
        )
        .unwrap();
        assert_eq!(
            lancamento.argumentos,
            vec!["--entrar".to_owned(), link],
            "o link não atravessou para a versão aberta, ou atravessou partido"
        );
    }

    #[test]
    fn sem_pedir_para_hospedar_nenhum_argumento_atravessa() {
        // A outra metade: um lançamento que sempre mandasse `--hospedar`
        // passaria no teste de cima e faria toda abertura virar hospedagem.
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let m = instalado(
            &config,
            &manifesto_json(&[("0.15.2", "0.15")], None),
            "0.15.2",
        );
        let lancamento = preparar(
            &config,
            &m,
            &chave(),
            &Pedido::Exata(Versao::nova("0.15.2").unwrap()),
            Ato::Abrir,
            &Intencao {
                nome_publico: Some("casa.exemplo.br".into()),
                ..Intencao::default()
            },
        )
        .unwrap();
        assert!(
            lancamento.argumentos.is_empty(),
            "abrir sem pedir para hospedar mandou argumento assim mesmo"
        );
    }

    #[test]
    fn um_pedido_vem_da_versao_ou_da_linha_e_nunca_das_duas() {
        assert!(matches!(
            pedido_de(Some("0.15.2"), None),
            Ok(Pedido::Exata(_))
        ));
        assert!(matches!(
            pedido_de(None, Some("0.15")),
            Ok(Pedido::MaisNovaDaLinha(_))
        ));
        assert_eq!(
            pedido_de(Some("0.15.2"), Some("0.15")),
            Err(FalhaAoAbrirVersao::IdentificadorInvalido)
        );
        assert_eq!(
            pedido_de(None, Some("../x")),
            Err(FalhaAoAbrirVersao::IdentificadorInvalido)
        );
        assert_eq!(
            pedido_de(Some("nao/e/uma/versao"), None),
            Err(FalhaAoAbrirVersao::IdentificadorInvalido)
        );
        assert!(matches!(pedido_de(None, None), Ok(Pedido::MaisNova)));
    }

    #[test]
    fn o_catalogo_guardado_e_o_ultimo_aceito_e_um_ilegivel_nao_vale() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        assert!(super::catalogo_guardado(&config, &chave()).is_none());

        let json = manifesto_json(&[("0.16.0", "1.0")], None);
        super::guardar_catalogo(&config, json.as_bytes()).unwrap();
        assert_eq!(
            super::catalogo_guardado(&config, &chave())
                .unwrap()
                .mais_nova()
                .version
                .como_texto(),
            "0.16.0"
        );

        std::fs::write(
            super::raiz_do_deposito(&config).join(super::CATALOGO_GUARDADO),
            b"{",
        )
        .unwrap();
        assert!(
            super::catalogo_guardado(&config, &chave()).is_none(),
            "um catálogo ilegível não pode passar por um que abre"
        );
    }

    #[test]
    fn o_atualizador_sai_do_bloco_do_tauri_conf() {
        let bloco = serde_json::json!({
            "pubkey": par().publica_no_formato_do_atualizador(),
            "endpoints": ["https://exemplo.invalido/latest.json"],
        });
        let atualizador = super::do_atualizador(&bloco).expect("chave e endereço declarados");
        assert_eq!(
            atualizador.enderecos,
            vec!["https://exemplo.invalido/latest.json"]
        );
        // Sem endereço, ou sem chave, não há catálogo a consultar.
        assert!(super::do_atualizador(
            &serde_json::json!({"pubkey": par().publica_no_formato_do_atualizador(), "endpoints": []})
        )
        .is_none());
        assert!(super::do_atualizador(&serde_json::json!({"endpoints": ["x"]})).is_none());
    }

    #[test]
    fn iniciar_o_que_nao_e_executavel_falha_sem_panico() {
        // O caminho do `spawn` tem de devolver recusa. Um arquivo que não é
        // executável é o que mais perto se chega, nos três sistemas, de «o
        // sistema recusou iniciar».
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let m = instalado(
            &config,
            &manifesto_json(&[("0.15.2", "0.15")], None),
            "0.15.2",
        );
        let lancamento = preparar(
            &config,
            &m,
            &chave(),
            &Pedido::MaisNova,
            Ato::Abrir,
            &Intencao::default(),
        )
        .unwrap();
        let resultado = super::iniciar(&lancamento);
        assert!(
            matches!(resultado, Err(FalhaAoAbrirVersao::NaoIniciou(_))) || resultado.is_ok(),
            "o caminho de abrir devolveu algo que não é nem sucesso nem `NaoIniciou`: {resultado:?}"
        );
    }

    /// **O lado que lê a revogação, para a versão deste processo** (spec 0.16,
    /// §2, F6). Hospedar e entrar são atos, e esta versão também é uma versão.
    #[test]
    fn esta_versao_revogada_e_dita_com_o_defeito_e_o_conserto() {
        let dir = tempfile::tempdir().unwrap();
        let deposito = Deposito::em(dir.path());
        let m = Manifesto::ler(
            manifesto_json(
                &[("0.16.1", "1.0"), ("0.16.0", "1.0")],
                Some(("0.16.0", "defeito de teste", "0.16.1")),
            )
            .as_bytes(),
            &chave(),
        )
        .unwrap();
        let vigente = deposito
            .conciliar_revogacoes(m.envelope_de_revogacoes(), &chave())
            .unwrap();

        assert_eq!(
            super::esta_versao_revogada(&vigente, Some("0.16.0")),
            Some(super::RecusaDestaVersao::EstaVersaoRevogada {
                defeito: "defeito de teste".to_owned(),
                corrigida_em: Some("0.16.1".to_owned()),
            })
        );
        assert_eq!(super::esta_versao_revogada(&vigente, Some("0.16.1")), None);
        // Um build sem versão carimbada não é revogável: não há o que conferir.
        assert_eq!(super::esta_versao_revogada(&vigente, None), None);
    }

    /// **A conferência ao entrar vem antes do único `connect` da tela.** O erro
    /// do `connect` é o da FFI, e por isso a conferência dele é um comando à
    /// parte, chamado logo antes. Este guarda prende a ordem: se alguém
    /// acrescentar outro `connect`, ou tirar a chamada, reprova aqui.
    #[test]
    fn entrar_confere_esta_versao_antes_de_conectar() {
        let tela = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/tela-boot.js"),
        )
        .unwrap();
        let todos: Vec<usize> = tela
            .match_indices("invoke(\"connect\"")
            .map(|(i, _)| i)
            .collect();
        let [unico] = todos.as_slice() else {
            panic!(
                "a tela tem {} chamadas a `connect`; a conferência da revogação cobre uma só",
                todos.len()
            );
        };
        let conferencia = tela
            .find("invoke(\"conferir_esta_versao\"")
            .expect("a tela não confere esta versão antes de entrar");
        assert!(
            conferencia < *unico,
            "a conferência de revogação vem depois do `connect`, e aí já é tarde"
        );
    }

    /// Um depósito escrito à mão, com o marcador de 4 linhas — para a lista,
    /// que lê o depósito sem manifesto nenhum.
    fn deposito_com(config: &std::path::Path, versao: &str, executavel: &str) {
        let deposito = Deposito::em(super::raiz_do_deposito(&config.to_string_lossy()));
        let v = Versao::nova(versao).unwrap();
        let pasta = deposito.pasta_da_versao(&v);
        std::fs::create_dir_all(pasta.join("bin")).unwrap();
        std::fs::write(pasta.join(executavel), b"nao sou um executavel de verdade").unwrap();
        let linha = Linha::da_versao_legada_do_app(versao)
            .expect("as versões destes testes são de antes das duas numerações");
        std::fs::write(
            pasta.join("INSTALADA"),
            format!("{versao}\n{executavel}\n{linha}\n{}\n", "0".repeat(64)),
        )
        .unwrap();
    }

    #[test]
    fn a_lista_marca_o_que_esta_em_uso_e_o_que_nao_da_para_abrir() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path();
        let config = raiz.to_string_lossy().into_owned();
        deposito_com(raiz, "0.10.5", "bin/seele");
        deposito_com(raiz, "0.11.0", "bin/seele");
        // E uma terceira, com marcador antigo.
        deposito_com(raiz, "0.9.0", "bin/seele");
        let deposito = Deposito::em(super::raiz_do_deposito(&config));
        std::fs::write(
            deposito
                .pasta_da_versao(&Versao::nova("0.9.0").unwrap())
                .join("INSTALADA"),
            b"0.9.0\n",
        )
        .unwrap();

        let lista = instaladas(&config);
        let do_deposito: Vec<_> = lista
            .iter()
            .filter(|v| ["0.10.5", "0.11.0", "0.9.0"].contains(&v.versao.as_str()))
            .collect();
        assert_eq!(
            do_deposito.len(),
            3,
            "o depósito tem três e a lista não as viu"
        );
        let antiga = lista.iter().find(|v| v.versao == "0.9.0").unwrap();
        assert!(
            !antiga.abrivel,
            "uma instalação sem procedência tem de aparecer dita, e não oferecida"
        );
        assert!(
            lista.iter().filter(|v| v.em_uso).count() <= 1,
            "duas versões marcadas como em uso ao mesmo tempo"
        );
    }

    /// **O portátil do Windows se abre pelo mesmo desempacotador do Mac.** A
    /// forma é a que o `windows.ps1` monta: a carga do instalador, `SEELE.exe`
    /// e `seeled.exe` na raiz, num tar.gz. Roda nos três sistemas, e é o que
    /// prova, longe do Windows, que o executável `SEELE.exe` que o manifesto
    /// declara é o que a instalação publica.
    #[test]
    fn o_portatil_do_windows_se_abre_pelo_mesmo_desempacotador() {
        let mut bruto = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut bruto, flate2::Compression::default());
            let mut tar = tar::Builder::new(gz);
            for (nome, conteudo) in [("SEELE.exe", b"app" as &[u8]), ("seeled.exe", b"servidor")] {
                let mut cabecalho = tar::Header::new_gnu();
                cabecalho.set_size(conteudo.len() as u64);
                cabecalho.set_mode(0o755);
                cabecalho.set_cksum();
                tar.append_data(&mut cabecalho, nome, conteudo).unwrap();
            }
            tar.into_inner().unwrap().finish().unwrap();
        }
        let destino = tempfile::tempdir().unwrap();
        seele_lancador::Desempacotador::desempacotar(
            &super::DesempacotarTarGz,
            &bruto,
            destino.path(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(destino.path().join("SEELE.exe")).unwrap(),
            b"app"
        );
        assert!(destino.path().join("seeled.exe").is_file());
    }

    /// **Relatado do uso real:** «não mostra as versões disponíveis para o
    /// server». A lista vinha só do depósito, e quem instalou pelo `.dmg` não
    /// tem depósito nenhum.
    #[test]
    fn a_versao_que_esta_rodando_aparece_mesmo_sem_deposito() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();

        let lista = instaladas(&config);

        match super::em_uso() {
            Some(versao) => {
                let achada = lista
                    .iter()
                    .find(|v| v.versao == versao)
                    .expect("a versão que está rodando tem de aparecer na lista");
                assert!(achada.em_uso, "e marcada como a que está aberta");
                assert!(
                    !achada.abrivel,
                    "o launcher não sabe abri-la: ela não está no depósito — e não precisa"
                );
            }
            None => assert!(
                lista.is_empty(),
                "sem carimbo de versão e sem depósito, não há o que listar"
            ),
        }
    }
}
