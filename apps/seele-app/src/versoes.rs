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
    /// O executável não é mais o que se instalou.
    ///
    /// A guarda da linha legada: o atualizador de uma versão aberta pelo
    /// launcher troca o app do depósito por outra linha. O conserto é apagar e
    /// baixar de novo, e a tela oferece isso num botão só.
    InstalacaoAlterada,
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
            Recusa::InstalacaoAlterada { .. } => Self::InstalacaoAlterada,
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

/// O maior esquema de banco que a 0.15.1 conhece.
///
/// Lido com `git show 54f542f:crates/seele-server/src/persistence/schema.rs`.
/// Um banco acima disto já foi migrado por uma versão mais nova, e não é dado
/// da 0.15: copiá-lo daria à 0.15.2 um banco do futuro, e o `migrate()` dela
/// não recusa (`persistence/mod.rs`, o levantamento de 08/10).
pub(crate) const ESQUEMA_DA_0_15: i64 = 13;

/// A linha legada, a única que o launcher oferece além da 1 (spec 0.16, §1).
fn linha_legada() -> Linha {
    // O texto é constante e canônico; o `expect` não tem como falhar, e um
    // teste prova (`a_primeira_abertura_guarda_os_dados_da_0_15_para_a_linha_dela`).
    #[allow(clippy::expect_used, reason = "texto constante, provado por teste")]
    Linha::nova("0.15").expect("«0.15» é uma linha")
}

/// Os bancos que a 0.15 escreve na pasta do app: o de sempre, o da identidade
/// do servidor, e um por servidor guardado.
fn bancos_da_pasta(config: &std::path::Path) -> Vec<PathBuf> {
    let mut bancos: Vec<PathBuf> = ["seele.db", "identidade-servidor.db"]
        .iter()
        .map(|nome| config.join(nome))
        .filter(|caminho| caminho.is_file())
        .collect();
    if let Ok(entradas) = std::fs::read_dir(config.join("servidores")) {
        bancos.extend(
            entradas
                .flatten()
                .map(|entrada| entrada.path().join("seele.db"))
                .filter(|caminho| caminho.is_file()),
        );
    }
    bancos
}

/// O esquema de um banco, aberto **só para leitura**.
///
/// Nada de `Persistence::open`, que migra: perguntar a idade de um banco não
/// pode mudar a idade dele.
fn esquema_do_banco(caminho: &std::path::Path) -> Option<i64> {
    let banco =
        rusqlite::Connection::open_with_flags(caminho, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    banco
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |linha| linha.get(0),
        )
        .ok()
}

/// O que a primeira abertura fez com os dados da 0.15.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum DadosDaLinhaLegada {
    /// A linha 0.15 já tinha os dados dela: uma abertura anterior copiou.
    JaGuardados,
    /// Copiados agora, com o tamanho e quanto custou.
    Guardados {
        /// Quantos arquivos.
        arquivos: u64,
        /// Quantos bytes.
        bytes: u64,
        /// Quanto tempo, em milissegundos.
        ms: u128,
    },
    /// A pasta do app não tem banco nenhum: não houve 0.15 nesta máquina.
    SemDados,
    /// A pasta tem um banco mais novo que a 0.15, e não é mais dado dela.
    BancoJaMigrado {
        /// O maior esquema achado.
        esquema: i64,
    },
    /// A cópia falhou, com o que o sistema disse.
    NaoGuardei(String),
}

impl DadosDaLinhaLegada {
    /// Houve 0.15 nesta máquina, e os dados dela estão na linha legada.
    ///
    /// É quem decide se a primeira abertura busca a linha legada: quem nunca
    /// teve 0.15 a baixa quando precisar dela, e não fala com a rede no
    /// arranque por isso.
    #[must_use]
    pub(crate) fn houve_0_15(&self) -> bool {
        matches!(self, Self::JaGuardados | Self::Guardados { .. })
    }
}

/// Spec 0.16, §1, «O que a atualização para a 0.16 precisa fazer na primeira
/// abertura», passo 1: copiar os dados da 0.15 para a linha dela **antes de
/// qualquer outra coisa** — sem `lancador/` e com o marcador de pronta.
///
/// Roda no `setup`, antes de a janela existir (ver
/// `a_janela_so_nasce_depois_da_primeira_abertura`) e antes da migração dos
/// MODs. Nunca falha para fora: o resultado é estado, e a tela o diz — o
/// «o produto sabe e não conta» do `CLAUDE.md` pelo outro lado.
pub(crate) fn primeira_abertura(config: &str) -> DadosDaLinhaLegada {
    let pasta = std::path::Path::new(config);
    let deposito = Deposito::em(raiz_do_deposito(config));
    let legada = linha_legada();
    if deposito.tem_dados_da_linha(&legada) {
        return DadosDaLinhaLegada::JaGuardados;
    }
    let bancos = bancos_da_pasta(pasta);
    if bancos.is_empty() {
        return DadosDaLinhaLegada::SemDados;
    }
    if let Some(esquema) = bancos.iter().filter_map(|b| esquema_do_banco(b)).max() {
        if esquema > ESQUEMA_DA_0_15 {
            tracing::warn!(
                esquema,
                "a pasta do app já tem banco mais novo que a 0.15; a linha legada abre vazia"
            );
            return DadosDaLinhaLegada::BancoJaMigrado { esquema };
        }
    }
    let inicio = std::time::Instant::now();
    match deposito.semear_linha(&legada, pasta, &["lancador"]) {
        Ok(seele_lancador::Semeadura::Semeada { arquivos, bytes }) => {
            let ms = inicio.elapsed().as_millis();
            tracing::info!(
                arquivos,
                bytes,
                ms,
                "dados da 0.15 guardados para a linha 0.15"
            );
            DadosDaLinhaLegada::Guardados {
                arquivos,
                bytes,
                ms,
            }
        }
        Ok(seele_lancador::Semeadura::JaTinha) => DadosDaLinhaLegada::JaGuardados,
        Err(erro) => {
            tracing::warn!(%erro, "não guardei os dados da 0.15 para a linha 0.15");
            DadosDaLinhaLegada::NaoGuardei(erro.to_string())
        }
    }
}

/// O que a primeira abertura fez, para a tela dizer uma vez.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PrimeiraAbertura {
    /// A cópia dos dados da 0.15.
    pub(crate) dados: DadosDaLinhaLegada,
    /// A busca da linha legada: `None` enquanto não terminou (ou não houve),
    /// e o que aconteceu quando terminou.
    pub(crate) linha_legada: Option<String>,
}

/// O estado da primeira abertura, guardado no app pelo `setup`.
#[derive(Debug, Default)]
pub(crate) struct EstadoDaPrimeiraAbertura(pub(crate) std::sync::Mutex<Option<PrimeiraAbertura>>);

/// O nome do arquivo que diz que a linha legada já foi buscada uma vez.
const LINHA_LEGADA_BUSCADA: &str = "linha-legada-buscada";

/// Passo 2 da primeira abertura: a linha legada para o depósito, **uma vez**.
///
/// Só onde houve 0.15 (ver [`DadosDaLinhaLegada::houve_0_15`]). O arquivo-marca
/// é escrito com o resultado, dando certo ou não: sem ele, uma máquina sem
/// rede falaria com o GitHub a cada arranque, que é o que o ADR 0026 recusa.
/// Sem rede agora, a linha é baixada quando alguém precisar dela — ao entrar
/// num servidor 0.15 ou ao hospedar nele.
pub(crate) async fn baixar_linha_legada_se_preciso(app: &tauri::AppHandle) -> Option<String> {
    let config = crate::config_dir(app);
    let marca = raiz_do_deposito(&config).join(LINHA_LEGADA_BUSCADA);
    if marca.exists() {
        return None;
    }
    let atualizador = atualizador_do_app(app)?;
    let resultado = match baixar(
        &config,
        &atualizador,
        &Pedido::MaisNovaDaLinha(linha_legada()),
    )
    .await
    {
        Ok(versao) => format!("guardada: {versao}"),
        Err(FalhaAoBaixarVersao::JaInstalada(versao)) => format!("guardada: {versao}"),
        Err(falha) => format!("não baixei: {falha:?}"),
    };
    tracing::info!(%resultado, "a linha legada na primeira abertura");
    if let Err(erro) = std::fs::create_dir_all(raiz_do_deposito(&config))
        .and_then(|()| std::fs::write(&marca, format!("{resultado}\n")))
    {
        tracing::warn!(%erro, "não marquei a busca da linha legada; o próximo arranque busca de novo");
    }
    Some(resultado)
}

/// A linha de servidor deste app: a do `VERSAO_DO_SERVIDOR` que o
/// `seele-proto` congelou (o app 0.16 fala o Servidor 1.0, da linha `1`).
#[must_use]
pub(crate) fn linha_deste_app() -> Option<Linha> {
    Linha::do_servidor(seele_ffi::VERSAO_DO_SERVIDOR).ok()
}

/// A pasta do app tem conversa? Há algum banco nela.
///
/// É a «conversa fora do depósito» do plano de dados: a linha que está aberta
/// guarda as conversas dela na pasta do app, e não no depósito.
#[must_use]
pub(crate) fn ha_conversa_na_pasta(config: &str) -> bool {
    !bancos_da_pasta(std::path::Path::new(config)).is_empty()
}

/// O que fazer com um link, antes de discar.
///
/// Spec 0.16, §1: ao entrar num servidor de outra linha, o launcher abre o app
/// mais novo daquela linha, e baixa antes se for preciso. A tela diz, antes,
/// que as conversas de uma linha não aparecem na outra.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum ParaEntrar {
    /// O servidor fala a linha deste app: entra aqui mesmo.
    Aqui,
    /// O link não diz a linha (sem `v=`, ou com um que não se lê). Tenta aqui;
    /// se for de outra linha, a recusa dirá.
    NaoSabe,
    /// Outra linha, e o app dela está guardado nesta máquina.
    Abrir {
        /// A linha, para pedir o app dela.
        linha: String,
        /// A versão do servidor, para a tela: «Servidor 0.15».
        servidor: String,
        /// O app que vai abrir.
        versao: String,
        /// A linha começa sem as conversas que há em outro lugar.
        perde_conversas: bool,
    },
    /// Outra linha, e o app dela precisa ser baixado antes.
    Baixar {
        /// A linha, para pedir o app dela.
        linha: String,
        /// A versão do servidor, para a tela.
        servidor: String,
        /// O catálogo guardado traz pacote para este sistema.
        tem_pacote: bool,
        /// A linha começa sem as conversas que há em outro lugar.
        perde_conversas: bool,
    },
    /// A linha do link deixou de ser oferecida (as 0.11 a 0.14, decisão de
    /// 08/10/2026), ou o catálogo não traz app nenhum dela.
    LinhaNaoOferecida {
        /// A linha que faltou.
        linha: String,
    },
}

/// O que fazer com o `v=` deste link, sabendo a linha deste app.
///
/// `M.N` é versão de servidor (o item 22 do contrato de fio); `M.N.P` é versão
/// de app, como os links da 0.15 trazem hoje, e vira linha pelo catálogo ou
/// pela regra de antes das duas numerações.
pub(crate) fn para_entrar_com_linha_propria(
    config: &str,
    chave: Option<&Chave>,
    versao_do_link: Option<&str>,
    propria: Option<Linha>,
) -> ParaEntrar {
    let Some(v) = versao_do_link else {
        return ParaEntrar::NaoSabe;
    };
    let manifesto = chave.and_then(|c| catalogo_guardado(config, c));
    let linha = Linha::do_servidor(v).ok().or_else(|| match &manifesto {
        Some(m) => m.linha_da_versao_do_app(v),
        None => Linha::da_versao_legada_do_app(v),
    });
    let (Some(linha), Some(propria)) = (linha, propria) else {
        return ParaEntrar::NaoSabe;
    };
    if linha == propria {
        return ParaEntrar::Aqui;
    }
    if linha_que_saiu(&linha) {
        return ParaEntrar::LinhaNaoOferecida {
            linha: linha.como_texto(),
        };
    }
    let perde_conversas = Deposito::em(raiz_do_deposito(config))
        .plano_da_linha(&linha, ha_conversa_na_pasta(config))
        .perde_conversas;
    let Some(manifesto) = manifesto else {
        return ParaEntrar::Baixar {
            servidor: linha.como_texto(),
            linha: linha.como_texto(),
            tem_pacote: false,
            perde_conversas,
        };
    };
    let Some(publicacao) = manifesto.mais_nova_da_linha(&linha) else {
        return ParaEntrar::LinhaNaoOferecida {
            linha: linha.como_texto(),
        };
    };
    let servidor = publicacao
        .unit
        .as_ref()
        .and_then(|u| u.server.clone())
        .unwrap_or_else(|| linha.como_texto());
    if Deposito::em(raiz_do_deposito(config))
        .instalacao(&publicacao.version)
        .is_some()
    {
        ParaEntrar::Abrir {
            linha: linha.como_texto(),
            servidor,
            versao: publicacao.version.como_texto().to_owned(),
            perde_conversas,
        }
    } else {
        ParaEntrar::Baixar {
            linha: linha.como_texto(),
            servidor,
            tem_pacote: publicacao.platforms.contains_key(&alvo_deste_sistema()),
            perde_conversas,
        }
    }
}

/// As linhas 0.11 a 0.14, que deixaram de ser oferecidas em 08/10/2026.
fn linha_que_saiu(linha: &Linha) -> bool {
    ["0.11", "0.12", "0.13", "0.14"].contains(&linha.como_texto().as_str())
}

/// O que fazer com o `v=` deste link, com a linha deste app.
pub(crate) fn para_entrar(
    config: &str,
    chave: Option<&Chave>,
    versao_do_link: Option<&str>,
) -> ParaEntrar {
    para_entrar_com_linha_propria(config, chave, versao_do_link, linha_deste_app())
}

/// Uma linha de servidor em que se pode hospedar, como a tela a desenha.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct LinhaParaHospedar {
    /// A linha, para pedir o app dela.
    pub(crate) linha: String,
    /// A versão do servidor, para a tela: «Servidor 0.15».
    pub(crate) servidor: String,
    /// As APIs de MOD que ela roda (a 0.15 roda 3, 4 e 5).
    pub(crate) apis: Vec<u32>,
    /// O app que hospeda nela, quando se sabe.
    pub(crate) app: Option<String>,
    /// É a linha desta janela: hospeda aqui mesmo, e é a padrão.
    pub(crate) esta: bool,
    /// O app dela está guardado (para a desta janela, sempre).
    pub(crate) instalada: bool,
    /// O catálogo traz pacote dela para este sistema.
    pub(crate) tem_pacote: bool,
    /// Começa sem as conversas que há em outro lugar.
    pub(crate) perde_conversas: bool,
}

/// As linhas em que se pode hospedar, a desta janela primeiro e as outras da
/// mais nova para a mais velha.
///
/// Sem rede: lê o catálogo guardado. A linha desta janela vem das constantes
/// compiladas, e não do catálogo, porque é este processo que hospeda nela —
/// mesmo que o catálogo já conheça um app mais novo dela.
pub(crate) fn linhas_para_hospedar_com_linha_propria(
    config: &str,
    chave: Option<&Chave>,
    propria: Option<Linha>,
) -> Vec<LinhaParaHospedar> {
    let mut linhas = Vec::new();
    if let Some(propria) = &propria {
        let mut apis: Vec<u32> = seele_ffi::mods::APIS_ACEITAS.to_vec();
        apis.sort_unstable();
        linhas.push(LinhaParaHospedar {
            linha: propria.como_texto(),
            servidor: seele_ffi::VERSAO_DO_SERVIDOR.to_owned(),
            apis,
            app: em_uso().map(str::to_owned),
            esta: true,
            instalada: true,
            tem_pacote: true,
            perde_conversas: false,
        });
    }
    let Some(manifesto) = chave.and_then(|c| catalogo_guardado(config, c)) else {
        return linhas;
    };
    let deposito = Deposito::em(raiz_do_deposito(config));
    let conversas_aqui = ha_conversa_na_pasta(config);
    for (linha, publicacao) in manifesto.linhas() {
        if propria.as_ref() == Some(&linha) {
            continue;
        }
        let unidade = publicacao.unit.as_ref();
        let mut apis = unidade
            .and_then(|u| u.mod_apis.clone())
            .or_else(|| unidade.map(|u| vec![u.mod_api]))
            .unwrap_or_default();
        apis.sort_unstable();
        linhas.push(LinhaParaHospedar {
            servidor: unidade
                .and_then(|u| u.server.clone())
                .unwrap_or_else(|| linha.como_texto()),
            apis,
            app: Some(publicacao.version.como_texto().to_owned()),
            esta: false,
            instalada: deposito.instalacao(&publicacao.version).is_some(),
            tem_pacote: publicacao.platforms.contains_key(&alvo_deste_sistema()),
            perde_conversas: deposito
                .plano_da_linha(&linha, conversas_aqui)
                .perde_conversas,
            linha: linha.como_texto(),
        });
    }
    linhas
}

/// As linhas em que se pode hospedar, com a linha deste app.
pub(crate) fn linhas_para_hospedar(config: &str, chave: Option<&Chave>) -> Vec<LinhaParaHospedar> {
    linhas_para_hospedar_com_linha_propria(config, chave, linha_deste_app())
}

/// Um app guardado, com o que ele ocupa.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct VersaoOcupada {
    /// Qual.
    pub(crate) versao: String,
    /// A linha dele, quando o marcador diz.
    pub(crate) linha: Option<String>,
    /// Quantos bytes.
    pub(crate) bytes: u64,
}

/// As conversas de uma linha, com o que elas ocupam.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct DadosOcupados {
    /// A linha.
    pub(crate) linha: String,
    /// Quantos bytes.
    pub(crate) bytes: u64,
}

/// Quanto o depósito ocupa, para a tela de disco.
///
/// ADR 0046: «passa a existir disco ocupado por versões guardadas, e uma tela
/// que diz quanto e permite apagar». As conversas aparecem separadas dos apps
/// de propósito: apagar um app não as leva.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Ocupacao {
    /// Tudo o que o depósito ocupa.
    pub(crate) total: u64,
    /// Os apps guardados, em ordem de nome.
    pub(crate) versoes: Vec<VersaoOcupada>,
    /// As conversas de cada linha, da linha mais nova para a mais velha.
    pub(crate) dados: Vec<DadosOcupados>,
}

/// O que o depósito ocupa agora.
pub(crate) fn ocupacao(config: &str) -> Ocupacao {
    let deposito = Deposito::em(raiz_do_deposito(config));
    let versoes: Vec<VersaoOcupada> = deposito
        .instaladas()
        .into_iter()
        .map(|versao| VersaoOcupada {
            linha: deposito.instalacao(&versao).map(|i| i.linha.como_texto()),
            bytes: deposito.tamanho_da_versao(&versao),
            versao: versao.como_texto().to_owned(),
        })
        .collect();
    // Toda pasta de dados que existe, com marcador ou sem: o que ocupa disco é
    // a pasta, e uma cópia que parou no meio também ocupa.
    let mut linhas: Vec<Linha> = std::fs::read_dir(raiz_do_deposito(config).join("dados"))
        .map(|entradas| {
            entradas
                .flatten()
                .filter_map(|e| {
                    e.file_name()
                        .to_str()
                        .and_then(|nome| nome.strip_prefix("servidor-"))
                        .and_then(|texto| Linha::nova(texto).ok())
                })
                .collect()
        })
        .unwrap_or_default();
    linhas.sort_by(|esta, aquela| aquela.cmp(esta));
    let dados: Vec<DadosOcupados> = linhas
        .iter()
        .map(|linha| DadosOcupados {
            linha: linha.como_texto(),
            bytes: deposito.tamanho_dos_dados_da_linha(linha),
        })
        .collect();
    let total =
        versoes.iter().map(|v| v.bytes).sum::<u64>() + dados.iter().map(|d| d.bytes).sum::<u64>();
    Ocupacao {
        total,
        versoes,
        dados,
    }
}

/// Por que um app guardado não foi apagado.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum FalhaAoRemoverVersao {
    /// O identificador não é uma versão.
    IdentificadorInvalido,
    /// Não está guardada.
    NaoInstalada,
    /// O sistema recusou; no Windows, quase sempre a versão está aberta.
    NaoRemovi(String),
}

/// Apaga um app guardado e devolve quantos bytes ele ocupava. **Nunca toca as
/// conversas**: elas são da linha, e o próximo app da mesma linha as abre.
///
/// # Errors
///
/// [`FalhaAoRemoverVersao`].
pub(crate) fn remover_versao(config: &str, versao: &str) -> Result<u64, FalhaAoRemoverVersao> {
    let versao = Versao::nova(versao).map_err(|_| FalhaAoRemoverVersao::IdentificadorInvalido)?;
    Deposito::em(raiz_do_deposito(config))
        .remover(&versao)
        .map_err(|falha| match falha {
            seele_lancador::FalhaAoRemover::NaoInstalada => FalhaAoRemoverVersao::NaoInstalada,
            seele_lancador::FalhaAoRemover::NaoRemovi { detalhe } => {
                FalhaAoRemoverVersao::NaoRemovi(detalhe)
            }
        })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório; e indexar um serde_json::Value devolve Null em vez de entrar em pânico"
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

    /// Uma pasta de app da 0.15: um banco com o esquema pedido e a identidade.
    /// O esquema é gravado como o `migrate()` grava: uma linha por migração,
    /// até a 13 da 0.15, e a pedida por último. Só as que precisa: um esquema
    /// «do futuro distante» não vira milhares de inserções.
    fn config_da_0_15(dir: &std::path::Path, esquema: i64) {
        let banco = rusqlite::Connection::open(dir.join("seele.db")).unwrap();
        banco
            .execute_batch(
                "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);",
            )
            .unwrap();
        for v in (1..=esquema.min(super::ESQUEMA_DA_0_15)).chain(std::iter::once(esquema)) {
            banco
                .execute("INSERT OR IGNORE INTO schema_version VALUES (?1, 0)", [v])
                .unwrap();
        }
        std::fs::write(dir.join("identity.key"), b"a chave da pessoa").unwrap();
    }

    /// **A ordem do spec 0.16, §1:** os dados da 0.15 viram os da linha 0.15
    /// antes de a 0.16 migrar a pasta dela, e a identidade vai junto.
    #[test]
    fn a_primeira_abertura_guarda_os_dados_da_0_15_para_a_linha_dela() {
        let dir = tempfile::tempdir().unwrap();
        config_da_0_15(dir.path(), super::ESQUEMA_DA_0_15);
        let config = dir.path().to_string_lossy().into_owned();

        let feito = super::primeira_abertura(&config);

        assert!(
            matches!(feito, super::DadosDaLinhaLegada::Guardados { .. }),
            "veio {feito:?}"
        );
        let deposito = Deposito::em(super::raiz_do_deposito(&config));
        assert!(deposito.tem_dados_da_linha(&linha("0.15")));
        let destino = deposito.pasta_de_dados_da_linha(&linha("0.15"));
        assert_eq!(
            std::fs::read(destino.join("identity.key")).unwrap(),
            b"a chave da pessoa",
            "a identidade não foi junto: a pessoa seria outra nos servidores da 0.15"
        );
        assert!(destino.join("seele.db").is_file());
        assert!(!destino.join("lancador").exists());
        // A segunda abertura não copia de novo.
        assert_eq!(
            super::primeira_abertura(&config),
            super::DadosDaLinhaLegada::JaGuardados
        );
    }

    /// Um banco que a 0.16 já migrou não é dado da 0.15: copiá-lo daria à
    /// 0.15.2 um banco do futuro, e o `migrate()` dela não recusa.
    ///
    /// **O 14 é o caso que importa, e fica**: é o esquema da 0.16 (a migração
    /// 14 da F1 põe `api` em `mods`), ou seja, a pasta que a 0.16 já migrou. Ele
    /// também é o que pega uma regressão que passasse a comparar com o esquema
    /// do próprio build — que agora é 14, e 14 > 14 é falso. O 9999 cobre o
    /// futuro distante, e sozinho não pegaria aquela regressão.
    #[test]
    fn um_banco_ja_migrado_nao_vira_dado_da_linha_legada() {
        for esquema in [super::ESQUEMA_DA_0_15 + 1, 9999] {
            let dir = tempfile::tempdir().unwrap();
            config_da_0_15(dir.path(), esquema);
            let config = dir.path().to_string_lossy().into_owned();
            assert_eq!(
                super::primeira_abertura(&config),
                super::DadosDaLinhaLegada::BancoJaMigrado { esquema },
                "com o esquema {esquema}"
            );
            assert!(
                !Deposito::em(super::raiz_do_deposito(&config)).tem_dados_da_linha(&linha("0.15")),
                "com o esquema {esquema}, um banco do futuro virou dado da linha 0.15"
            );
        }
    }

    /// O banco de um servidor guardado também conta: um só migrado já basta
    /// para a pasta não ser mais da 0.15.
    #[test]
    fn um_banco_de_servidor_guardado_ja_migrado_tambem_conta() {
        let dir = tempfile::tempdir().unwrap();
        config_da_0_15(dir.path(), super::ESQUEMA_DA_0_15);
        let outro = dir.path().join("servidores/abc");
        std::fs::create_dir_all(&outro).unwrap();
        config_da_0_15(&outro, super::ESQUEMA_DA_0_15 + 2);
        let config = dir.path().to_string_lossy().into_owned();
        assert!(matches!(
            super::primeira_abertura(&config),
            super::DadosDaLinhaLegada::BancoJaMigrado { .. }
        ));
    }

    #[test]
    fn uma_maquina_sem_0_15_nao_copia_nada() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        assert_eq!(
            super::primeira_abertura(&config),
            super::DadosDaLinhaLegada::SemDados
        );
        assert!(!super::raiz_do_deposito(&config).join("dados").exists());
    }

    /// **O guarda estrutural.** No Tauri 2.11.6 as janelas do `tauri.conf.json`
    /// nascem antes do `setup` do app (`app.rs:2521-2531`). A janela é
    /// `create: false`, e o `setup` copia os dados da 0.15 antes de criá-la.
    #[test]
    fn a_janela_so_nasce_depois_da_primeira_abertura() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            conf["app"]["windows"][0]["create"], false,
            "a janela nasce sozinha, antes do setup e da cópia dos dados da 0.15"
        );
        let main = include_str!("main.rs");
        let setup = main.find(".setup(move |app|").expect("o setup do app");
        let copia = main[setup..]
            .find("versoes::primeira_abertura(")
            .expect("o setup chama a primeira abertura");
        let janela = main[setup..]
            .find("WebviewWindowBuilder::from_config(")
            .expect("o setup cria a janela");
        let mods = main[setup..]
            .find("mods::levar_pacotes_antigos_para_o_cache(")
            .expect("o setup migra os MODs");
        assert!(
            copia < janela,
            "a janela é criada antes da cópia dos dados da 0.15"
        );
        assert!(
            copia < mods,
            "os MODs da pasta são migrados antes da cópia: a linha 0.15 ficaria com o layout novo"
        );
    }

    /// 13 é o maior esquema que a 0.15.1 conhece, lido com
    /// `git show 54f542f:crates/seele-server/src/persistence/schema.rs`. Este
    /// teste não prova isso (o histórico não está aqui); ele prende o número,
    /// para uma mudança nele ser uma decisão escrita.
    #[test]
    fn o_esquema_da_0_15_e_o_da_0_15_1() {
        assert_eq!(super::ESQUEMA_DA_0_15, 13);
    }

    /// **O critério da F6:** um link de um servidor 0.15 (o `v=0.15.1` que a
    /// 0.15 emite hoje) abre o app da linha 0.15 quando ele está guardado.
    #[test]
    fn um_link_da_0_15_abre_a_linha_legada_quando_ela_esta_guardada() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let json = manifesto_json(&[("0.16.0", "1.0"), ("0.15.2", "0.15")], None);
        instalado(&config, &json, "0.15.2");
        super::guardar_catalogo(&config, json.as_bytes()).unwrap();

        let decisao = super::para_entrar_com_linha_propria(
            &config,
            Some(&chave()),
            Some("0.15.1"),
            Some(linha("1")),
        );
        assert_eq!(
            decisao,
            super::ParaEntrar::Abrir {
                linha: "0.15".to_owned(),
                servidor: "0.15".to_owned(),
                versao: "0.15.2".to_owned(),
                perde_conversas: false,
            }
        );
    }

    #[test]
    fn um_link_de_outra_linha_que_nao_esta_aqui_pede_para_baixar() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let json = manifesto_json(&[("0.16.0", "1.0"), ("0.15.2", "0.15")], None);
        super::guardar_catalogo(&config, json.as_bytes()).unwrap();
        // A pasta do app tem conversa: abrir a 0.15 começa sem ela.
        std::fs::write(dir.path().join("seele.db"), b"x").unwrap();
        assert_eq!(
            super::para_entrar_com_linha_propria(
                &config,
                Some(&chave()),
                Some("0.15.1"),
                Some(linha("1"))
            ),
            super::ParaEntrar::Baixar {
                linha: "0.15".to_owned(),
                servidor: "0.15".to_owned(),
                tem_pacote: true,
                perde_conversas: true,
            }
        );
    }

    #[test]
    fn um_link_da_mesma_linha_entra_aqui_mesmo() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let json = manifesto_json(&[("0.16.1", "1.0"), ("0.16.0", "1.0")], None);
        super::guardar_catalogo(&config, json.as_bytes()).unwrap();
        // `v=1.0` (a versão do servidor, depois do item 22 do contrato) e
        // `v=0.16.0` (a versão do app, como os links da 0.16 antes dele).
        for v in ["1.0", "1.3", "0.16.0"] {
            assert_eq!(
                super::para_entrar_com_linha_propria(
                    &config,
                    Some(&chave()),
                    Some(v),
                    Some(linha("1"))
                ),
                super::ParaEntrar::Aqui,
                "com v={v}"
            );
        }
    }

    #[test]
    fn um_link_de_uma_linha_que_saiu_diz_isso() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        super::guardar_catalogo(
            &config,
            manifesto_json(&[("0.16.0", "1.0"), ("0.15.2", "0.15")], None).as_bytes(),
        )
        .unwrap();
        assert_eq!(
            super::para_entrar_com_linha_propria(
                &config,
                Some(&chave()),
                Some("0.14.2"),
                Some(linha("1"))
            ),
            super::ParaEntrar::LinhaNaoOferecida {
                linha: "0.14".to_owned()
            }
        );
    }

    /// Sem o catálogo guardado, a linha do link ainda é sabida pela regra
    /// legada, e a resposta é baixar — sem dizer que há pacote, que só o
    /// catálogo sabe.
    #[test]
    fn sem_catalogo_a_linha_do_link_ainda_e_sabida() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        assert_eq!(
            super::para_entrar_com_linha_propria(
                &config,
                Some(&chave()),
                Some("0.15.1"),
                Some(linha("1"))
            ),
            super::ParaEntrar::Baixar {
                linha: "0.15".to_owned(),
                servidor: "0.15".to_owned(),
                tem_pacote: false,
                perde_conversas: false,
            }
        );
    }

    #[test]
    fn sem_v_no_link_ninguem_adivinha() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        assert_eq!(
            super::para_entrar_com_linha_propria(&config, None, None, Some(linha("1"))),
            super::ParaEntrar::NaoSabe
        );
        assert_eq!(
            super::para_entrar_com_linha_propria(&config, None, Some("lixo"), Some(linha("1"))),
            super::ParaEntrar::NaoSabe
        );
    }

    /// O manifesto de teste com as APIs de cada versão escolhidas.
    fn manifesto_json_com_apis(versoes: &[(&str, &str, &[u32])]) -> String {
        let alvo = super::alvo_deste_sistema();
        let entradas: Vec<String> = versoes
            .iter()
            .map(|(v, servidor, apis)| {
                let bytes = pacote("bin/seele", v.as_bytes());
                let lista: Vec<String> = apis.iter().map(u32::to_string).collect();
                format!(
                    r#"{{"version":"{v}","unit":{{"protocol":8,"mod_api":{},"server":"{servidor}","mod_apis":[{}]}},
                        "platforms":{{"{alvo}":{{"url":"https://exemplo.invalido/{v}","signature":"{}",
                        "sha256":"{}","executable":"bin/seele"}}}}}}"#,
                    apis.iter().max().copied().unwrap_or(0),
                    lista.join(","),
                    par().assinar_no_formato_do_atualizador(&bytes),
                    soma(&bytes),
                )
            })
            .collect();
        format!(r#"{{"schema":2,"versions":[{}]}}"#, entradas.join(","))
    }

    /// **Spec 0.16, §1:** ao hospedar, escolhe-se a versão do servidor; a
    /// padrão é a deste app, e a tela diz quais APIs de MOD cada uma roda.
    #[test]
    fn hospedar_lista_cada_linha_com_as_apis_e_a_desta_janela_primeiro() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let json = manifesto_json_com_apis(&[
            ("0.16.1", "1.0", &[6]),
            ("0.16.0", "1.0", &[6]),
            ("0.15.2", "0.15", &[3, 4, 5]),
        ]);
        super::guardar_catalogo(&config, json.as_bytes()).unwrap();

        let linhas = super::linhas_para_hospedar_com_linha_propria(
            &config,
            Some(&chave()),
            Some(linha("1")),
        );

        assert_eq!(linhas.len(), 2, "veio {linhas:?}");
        assert_eq!(linhas[0].linha, "1");
        assert!(
            linhas[0].esta,
            "a linha desta janela é a padrão, e vem primeiro"
        );
        assert!(linhas[0].instalada, "a desta janela hospeda aqui mesmo");
        assert_eq!(linhas[1].linha, "0.15");
        assert_eq!(linhas[1].servidor, "0.15");
        assert_eq!(
            linhas[1].apis,
            vec![3, 4, 5],
            "a tela diz quais MODs a 0.15 roda"
        );
        assert_eq!(linhas[1].app.as_deref(), Some("0.15.2"));
        assert!(!linhas[1].esta);
        assert!(!linhas[1].instalada && linhas[1].tem_pacote);
    }

    /// Sem catálogo guardado, só a linha desta janela: as outras aparecem
    /// depois de o catálogo ser lido uma vez.
    #[test]
    fn sem_catalogo_hospedar_oferece_so_a_linha_desta_janela() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let linhas = super::linhas_para_hospedar_com_linha_propria(
            &config,
            Some(&chave()),
            Some(linha("1")),
        );
        assert_eq!(linhas.len(), 1);
        assert!(linhas[0].esta);
        assert_eq!(linhas[0].servidor, seele_ffi::VERSAO_DO_SERVIDOR);
    }

    /// A tela de preparar a hospedagem lista por linha e abre por linha. Guarda
    /// de texto: prende o comando que a lista lê e o pedido que o confirmar
    /// manda.
    #[test]
    fn preparar_a_hospedagem_e_por_linha() {
        let tela = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/tela-boot.js"),
        )
        .unwrap();
        assert!(
            tela.contains("invoke(\"linhas_para_hospedar\")"),
            "a tela de preparar não lista as versões do servidor"
        );
        // Sem espaços, para o guarda não depender da formatação; e na chamada
        // de `abrir_versao`, porque a de `baixar_versao` também leva a linha e
        // esconderia um confirmar que abrisse pela versão.
        let compacta: String = tela.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            compacta.contains(
                "invoke(\"abrir_versao\",{versao:null,linha:linhaEscolhida.linha,hospedar:true"
            ),
            "o confirmar não abre pela linha escolhida"
        );
    }

    /// **A tela de disco mínima** (spec 0.16, §2, F6): quanto cada app e as
    /// conversas de cada linha ocupam, e apagar um app sem levar as conversas.
    #[test]
    fn a_ocupacao_soma_versoes_e_dados_e_remover_nao_leva_as_conversas() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let json = manifesto_json(&[("0.16.0", "1.0"), ("0.15.2", "0.15")], None);
        instalado(&config, &json, "0.15.2");
        let deposito = Deposito::em(super::raiz_do_deposito(&config));
        let dados = deposito.pasta_de_dados_da_linha(&linha("0.15"));
        std::fs::create_dir_all(&dados).unwrap();
        std::fs::write(dados.join("seele.db"), vec![0_u8; 4096]).unwrap();

        let antes = super::ocupacao(&config);
        assert_eq!(antes.versoes.len(), 1);
        assert_eq!(antes.versoes[0].versao, "0.15.2");
        assert_eq!(antes.versoes[0].linha.as_deref(), Some("0.15"));
        assert!(antes.versoes[0].bytes > 0);
        assert_eq!(
            antes.dados,
            vec![super::DadosOcupados {
                linha: "0.15".to_owned(),
                bytes: 4096
            }]
        );
        assert!(antes.total >= 4096 + antes.versoes[0].bytes);

        assert!(super::remover_versao(&config, "0.15.2").unwrap() > 0);
        let depois = super::ocupacao(&config);
        assert!(depois.versoes.is_empty());
        assert_eq!(
            depois.dados, antes.dados,
            "remover o app levou as conversas"
        );
        assert_eq!(
            super::remover_versao(&config, "0.15.2"),
            Err(super::FalhaAoRemoverVersao::NaoInstalada)
        );
        assert_eq!(
            super::remover_versao(&config, "../fuga"),
            Err(super::FalhaAoRemoverVersao::IdentificadorInvalido)
        );
    }

    /// **O painel de servidores segue a decisão do Rust.** Um guarda de texto,
    /// e por isso limitado: ele prende que o campo é lido e que a decisão vai
    /// para `entrarPelaLinha`. O campo antigo, que só abria a versão exata já
    /// instalada, não pode voltar.
    #[test]
    fn o_painel_de_servidores_segue_a_decisao_de_entrar() {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let servidores = std::fs::read_to_string(ui.join("camada-servidores.js")).unwrap();
        assert!(
            servidores.contains("convite.para_entrar") && servidores.contains("entrarPelaLinha("),
            "o painel de servidores não segue a decisão de entrar por linha"
        );
        assert!(
            !servidores.contains("pode_abrir_naquela_versao"),
            "o campo antigo voltou ao painel de servidores"
        );
        let versoes = std::fs::read_to_string(ui.join("camada-versoes.js")).unwrap();
        assert!(
            versoes.contains("async function entrarPelaLinha("),
            "`entrarPelaLinha` sumiu de camada-versoes.js"
        );
    }

    /// A linha deste app sai da constante que a F1 congelou: o app 0.16 fala o
    /// Servidor 1.0, da linha `1`.
    #[test]
    fn a_linha_deste_app_e_a_do_servidor_que_ele_fala() {
        assert_eq!(super::linha_deste_app(), Some(linha("1")));
    }

    /// **A guarda da linha legada, na casca.** O atualizador de uma versão
    /// aberta pelo launcher trocaria o executável do depósito; abrir de novo
    /// recusa com o nome certo, para a tela oferecer baixar outra vez.
    #[test]
    fn um_executavel_trocado_no_deposito_e_recusado_com_o_nome_certo() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().to_string_lossy().into_owned();
        let m = instalado(
            &config,
            &manifesto_json(&[("0.15.2", "0.15")], None),
            "0.15.2",
        );
        let executavel = Deposito::em(super::raiz_do_deposito(&config))
            .pasta_da_versao(&Versao::nova("0.15.2").unwrap())
            .join("bin/seele");
        std::fs::write(&executavel, b"o app de outra linha").unwrap();

        assert_eq!(
            preparar(
                &config,
                &m,
                &chave(),
                &Pedido::MaisNovaDaLinha(linha("0.15")),
                Ato::Abrir,
                &Intencao::default()
            ),
            Err(FalhaAoAbrirVersao::InstalacaoAlterada)
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
