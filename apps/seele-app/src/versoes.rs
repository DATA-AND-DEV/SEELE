//! O lado da casca do launcher: que versões há nesta máquina, e abrir uma.
//!
//! # O que este módulo é, e o que ele não é
//!
//! O [ADR 0046](../../../docs/adr/0046-toda-versao-continua-de-pe.md) decidiu
//! que o app vira launcher: *«cliente e servidor são versionados juntos, e ao
//! conectar o cliente roda a versão daquele servidor»*. O núcleo dessa decisão
//! é o `seele-lancador`, que é biblioteca pura — ele não fala com a rede e não
//! desempacota, de propósito.
//!
//! Este módulo é o **chamador** que faltava. Até aqui o `seele-lancador`
//! existia, passava noventa testes, e **nenhum crate dependia dele**: ele não
//! tinha binário e ninguém o chamava. É o caso do `CLAUDE.md` com todas as
//! letras — «existir não é funcionar» —, e cinco mil linhas verdes e
//! inalcançáveis são a forma mais cara dele.
//!
//! # O que fica de fora, e é dito em voz alta
//!
//! **Baixar uma versão que não está instalada.** Isso precisa do manifesto, que
//! vem da rede, e é trabalho do atualizador do ADR 0026. O que está aqui
//! funciona **offline** e cobre o caso que o dono pediu: *«o host quer usar mod
//! x e mod y, só que esses mods foram feitos para uma versão anterior; quando
//! for hospedar, pode escolher a versão do server que é compatível»* — e quem
//! escolhe uma versão anterior por causa de um MOD já a tem no disco.
//!
//! Por isso a lista daqui é a do **depósito**, e não a do manifesto. Ela nunca
//! diz «esta é a mais nova»: quem sabe isso é o manifesto, e ele não está aqui.

use std::path::PathBuf;

use seele_lancador::{Deposito, Lancamento, Versao};

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
    deposito
        .instaladas()
        .into_iter()
        .map(|versao| VersaoInstalada {
            em_uso: agora == Some(versao.como_texto()),
            abrivel: deposito.instalacao(&versao).is_some(),
            versao: versao.como_texto().to_owned(),
        })
        .collect()
}

/// Por que não deu para abrir a versão pedida.
///
/// Nome estável e não frase, como as outras recusas que cruzam esta ponte: a
/// frase mora no `FRASES` do JavaScript.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) enum FalhaAoAbrirVersao {
    /// O identificador não é uma versão.
    IdentificadorInvalido,
    /// Não está instalada nesta máquina.
    ///
    /// Baixá-la é trabalho do atualizador e precisa da rede; este caminho é
    /// offline de propósito.
    NaoInstalada,
    /// Está instalada e não sabe dizer o que rodar.
    ///
    /// O marcador dela é anterior à anotação do executável, ou o arquivo sumiu
    /// depois de instalado. Separada de [`Self::NaoInstalada`] porque o conserto
    /// é outro: aqui a versão está lá e o que falta é reinstalá-la.
    SemProcedencia,
    /// O sistema recusou iniciar o processo, e o que ele disse.
    NaoIniciou(String),
}

/// Abre uma versão instalada, com os dados dela.
///
/// # O que «com os dados dela» quer dizer
///
/// Cada versão tem o próprio diretório, e quem o entrega é o `SEELE_HOME` que o
/// produto **já** obedece — nenhum mecanismo novo. O passo mais curto do
/// launcher é também o mais fácil de errar calado: um processo iniciado com o
/// executável certo e o diretório errado abre, funciona, e escreve as conversas
/// no lugar de outra versão, sem sintoma nenhum até alguém procurar o que
/// escreveu.
///
/// # Errors
///
/// [`FalhaAoAbrirVersao`], uma variante por motivo.
pub(crate) fn abrir(
    config: &str,
    versao: &str,
    hospedar: bool,
    nome_publico: Option<&str>,
    link: Option<&str>,
) -> Result<(), FalhaAoAbrirVersao> {
    preparar(config, versao, hospedar, nome_publico, link)?
        .iniciar()
        .map(|_| ())
        .map_err(|erro| FalhaAoAbrirVersao::NaoIniciou(erro.to_string()))
}

/// O lançamento montado, sem iniciar processo nenhum.
///
/// Separado de [`abrir`] pela mesma razão que o `seele-lancador` separa
/// `Lancamento` de `Lancamento::iniciar`: **é o que permite provar, sem abrir
/// processo, que a versão escolhida vai receber o diretório dela**. Um processo
/// iniciado com o executável certo e os dados errados abre, funciona, e escreve
/// as conversas no lugar de outra versão — sem sintoma até alguém procurar o
/// que escreveu.
///
/// E provar o lançamento em vez do processo é o que faz o teste rodar nos três
/// sistemas. Um teste que precisasse executar alguma coisa precisaria de um
/// programa que existisse nos três, e acabaria atrás de um `cfg` — que é o
/// «existir não é funcionar» de novo, uma camada abaixo.
///
/// # Errors
///
/// [`FalhaAoAbrirVersao`], menos [`FalhaAoAbrirVersao::NaoIniciou`], que só
/// existe depois de tentar.
fn preparar(
    config: &str,
    versao: &str,
    hospedar: bool,
    nome_publico: Option<&str>,
    link: Option<&str>,
) -> Result<Lancamento, FalhaAoAbrirVersao> {
    let versao = Versao::nova(versao).map_err(|_| FalhaAoAbrirVersao::IdentificadorInvalido)?;
    let deposito = Deposito::em(raiz_do_deposito(config));
    if !deposito.esta_instalada(&versao) {
        return Err(FalhaAoAbrirVersao::NaoInstalada);
    }
    let instalacao = deposito
        .instalacao(&versao)
        .ok_or(FalhaAoAbrirVersao::SemProcedencia)?;

    // Os argumentos são repassados como vieram, e uma versão que não os conhece
    // os ignora — é o que faz este caminho valer também para as publicadas
    // antes de o launcher existir. Ela abre; quem aperta o botão é a pessoa.
    let mut argumentos = Vec::new();
    if hospedar {
        argumentos.push("--hospedar".to_owned());
        if let Some(nome) = nome_publico {
            argumentos.push("--nome-publico".to_owned());
            argumentos.push(nome.to_owned());
        }
    }
    // O outro motivo de abrir outra versão, e o que o ADR 0046 quer de
    // verdade: **entrar** num servidor que roda aquela versão. O link vai
    // inteiro — ele carrega a impressão digital, os endereços alternativos e o
    // convite de uso único, e reconstruí-lo do outro lado a partir de pedaços
    // seria um segundo analisador para discordar do primeiro.
    if let Some(link) = link {
        argumentos.push("--entrar".to_owned());
        argumentos.push(link.to_owned());
    }

    let resolvida = seele_lancador::VersaoResolvida {
        versao: instalacao.versao,
        executavel: instalacao.executavel,
        dados: deposito.pasta_de_dados(&versao),
        unidade: None,
    };
    // A pasta de dados precisa existir antes: o produto a cria sozinho, mas
    // criá-la aqui é o que faz o erro — um disco cheio, uma permissão — chegar
    // como recusa desta função em vez de como uma janela que abre vazia.
    if let Err(erro) = std::fs::create_dir_all(&resolvida.dados) {
        return Err(FalhaAoAbrirVersao::NaoIniciou(erro.to_string()));
    }

    Ok(Lancamento::de(&resolvida, argumentos))
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
    /// No Windows isto é o estado normal e não um defeito: o que o catálogo
    /// publica ali é um **instalador**, que instala por cima da instalação
    /// única — não um pacote que se abra ao lado de outro. Ver a nota em
    /// [`desempacotar_alvo`].
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
/// # O que este formato cobre, e o que ele não cobre
///
/// O `.app.tar.gz` do macOS e o pacote do Linux — que são o que o catálogo
/// publica para o atualizador, e são arquivos que se abrem ao lado de outro.
///
/// **O Windows não passa por aqui, e é honesto dizer por quê.** O que o
/// catálogo publica ali é um `.exe` de instalação: ele instala por cima da
/// instalação única da máquina, mexe no registro e no menu iniciar. Abrir isso
/// «ao lado» não é uma questão de formato — é uma coisa que aquele artefato não
/// sabe ser. Versões lado a lado no Windows precisam de um pacote próprio no
/// catálogo, e ele não existe ainda.
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

/// Baixa do catálogo a versão mais nova e a instala ao lado das outras.
///
/// # A ordem, e por que ela é esta
///
/// Catálogo, conferência do catálogo, pacote, conferência do pacote, e só então
/// disco. **Nada é escrito antes de a assinatura passar** — é o contrato do
/// `Deposito::instalar`, e o motivo é o mesmo de sempre: meia instalação é o
/// estado que ninguém sabe desfazer.
///
/// # Errors
///
/// [`FalhaAoBaixarVersao`], uma variante por motivo.
pub(crate) async fn baixar_a_mais_nova(
    config: &str,
    pubkey: &str,
    enderecos: &[String],
) -> Result<String, FalhaAoBaixarVersao> {
    let chave = seele_lancador::Chave::do_formato_do_atualizador(pubkey)
        .map_err(|erro| FalhaAoBaixarVersao::CatalogoNaoServe(format!("{erro:?}")))?;

    let cliente = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|erro| FalhaAoBaixarVersao::NaoBaixei(erro.to_string()))?;

    // Os endereços em ordem, como o atualizador os tenta: o primeiro que
    // responde vale. Um espelho existe justamente para o dia em que o outro
    // está fora.
    let mut bytes_do_catalogo = None;
    for endereco in enderecos {
        if let Ok(resposta) = cliente.get(endereco).send().await {
            if resposta.status().is_success() {
                if let Ok(bytes) = resposta.bytes().await {
                    bytes_do_catalogo = Some(bytes.to_vec());
                    break;
                }
            }
        }
    }
    let Some(bytes_do_catalogo) = bytes_do_catalogo else {
        return Err(FalhaAoBaixarVersao::CatalogoNaoRespondeu);
    };

    let manifesto = seele_lancador::Manifesto::ler(&bytes_do_catalogo, &chave)
        .map_err(|erro| FalhaAoBaixarVersao::CatalogoNaoServe(format!("{erro:?}")))?;
    let publicacao = manifesto.mais_nova().clone();
    let versao_texto = publicacao.version.como_texto().to_owned();

    let deposito = Deposito::em(raiz_do_deposito(config));
    if deposito.esta_instalada(&publicacao.version) {
        return Err(FalhaAoBaixarVersao::JaInstalada(versao_texto));
    }

    let alvo = alvo_deste_sistema();
    let Some(pacote) = publicacao.platforms.get(&alvo) else {
        return Err(FalhaAoBaixarVersao::SemPacoteParaEsteSistema(alvo));
    };
    let Some(url) = pacote.url() else {
        return Err(FalhaAoBaixarVersao::SemPacoteParaEsteSistema(alvo));
    };

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
    tauri::async_runtime::spawn_blocking(move || {
        Deposito::em(raiz).instalar(&publicacao, &alvo, &bytes, &chave, &DesempacotarTarGz)
    })
    .await
    .map_err(|erro| FalhaAoBaixarVersao::NaoBaixei(erro.to_string()))?
    .map_err(|erro| FalhaAoBaixarVersao::PacoteRecusado(format!("{erro:?}")))?;

    Ok(versao_texto)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod o_launcher_ligado_ao_produto {
    use super::{abrir, instaladas, preparar, FalhaAoAbrirVersao};
    use seele_lancador::{Deposito, Versao};

    /// Um depósito de mentira com uma versão instalada de verdade.
    ///
    /// Escrito à mão, e não pelo `Deposito::instalar`: instalar exige pacote
    /// assinado, manifesto e desempacotador, e o que se quer provar aqui é o
    /// **consumo** do depósito pela casca, não a instalação. Escrever o que uma
    /// instalação deixa em disco é o contrato entre os dois, e é ele que este
    /// teste prende — se o formato do marcador mudar sem que este arquivo saiba,
    /// é aqui que aparece.
    fn deposito_com(config: &std::path::Path, versao: &str, executavel: &str) {
        let deposito = Deposito::em(super::raiz_do_deposito(&config.to_string_lossy()));
        let v = Versao::nova(versao).unwrap();
        let pasta = deposito.pasta_da_versao(&v);
        std::fs::create_dir_all(pasta.join("bin")).unwrap();
        std::fs::write(pasta.join(executavel), b"nao sou um executavel de verdade").unwrap();
        std::fs::write(pasta.join("INSTALADA"), format!("{versao}\n{executavel}\n")).unwrap();
    }

    fn pasta(nome: &str) -> std::path::PathBuf {
        let caminho = std::env::temp_dir().join(format!(
            "seele-versoes-{}-{}-{nome}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&caminho).unwrap();
        caminho
    }

    #[test]
    fn a_versao_escolhida_recebe_o_executavel_dela_e_os_dados_dela() {
        // **O guarda contra o defeito que não tem sintoma.** Um processo aberto
        // com o executável certo e o `SEELE_HOME` errado funciona, e escreve as
        // conversas no diretório de outra versão. Ninguém descobre até trocar de
        // versão e não achar o que escreveu.
        let raiz = pasta("caminho-completo");
        let config = raiz.to_string_lossy().into_owned();
        deposito_com(&raiz, "0.10.5", "bin/seele");

        let lancamento = preparar(&config, "0.10.5", true, Some("casa.exemplo.br"), None)
            .expect("uma versão instalada e com procedência tem de preparar");

        let deposito = Deposito::em(super::raiz_do_deposito(&config));
        let v = Versao::nova("0.10.5").unwrap();
        assert_eq!(
            lancamento.programa,
            deposito.pasta_da_versao(&v).join("bin/seele"),
            "o programa não é o executável que aquela instalação publicou"
        );
        assert_eq!(
            lancamento.ambiente,
            vec![(
                "SEELE_HOME".to_owned(),
                deposito.pasta_de_dados(&v).to_string_lossy().into_owned()
            )],
            "a versão foi aberta sem o diretório de dados dela, ou com o de outra"
        );
        assert!(
            deposito.pasta_de_dados(&v).is_dir(),
            "a pasta de dados não foi criada antes de o processo subir"
        );
        assert_eq!(
            lancamento.argumentos,
            vec![
                "--hospedar".to_owned(),
                "--nome-publico".to_owned(),
                "casa.exemplo.br".to_owned()
            ],
            "a intenção de hospedar, ou o nome público, não atravessou para a \
             versão aberta — e a tela prometeu que atravessaria"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn entrar_num_servidor_leva_o_link_inteiro_para_a_versao_aberta() {
        // **O coração do ADR 0046**: quem clica num `seele://` de um servidor
        // que roda outra versão tem aquela versão aberta, já indo para lá.
        //
        // O link vai **inteiro**, e não em pedaços: ele carrega a impressão
        // digital, os endereços alternativos e o convite de uso único.
        // Remontá-lo do outro lado a partir de campos seria um segundo
        // analisador de URI para discordar do primeiro — e o que se perderia
        // primeiro é justamente a impressão digital, que é a única coisa que
        // prova que o servidor do outro lado é o do link.
        let raiz = pasta("entrar");
        let config = raiz.to_string_lossy().into_owned();
        deposito_com(&raiz, "0.10.5", "bin/seele");
        let link = format!("seele://casa.exemplo:8383/?fp={}", "a".repeat(64));

        let lancamento = preparar(&config, "0.10.5", false, None, Some(&link)).expect("preparar");
        assert_eq!(
            lancamento.argumentos,
            vec!["--entrar".to_owned(), link.clone()],
            "o link não atravessou para a versão aberta, ou atravessou partido"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn sem_pedir_para_hospedar_nenhum_argumento_atravessa() {
        // A outra metade: um lançamento que sempre mandasse `--hospedar`
        // passaria no teste de cima e faria toda abertura virar hospedagem.
        let raiz = pasta("so-abrir");
        let config = raiz.to_string_lossy().into_owned();
        deposito_com(&raiz, "0.10.5", "bin/seele");

        let lancamento =
            preparar(&config, "0.10.5", false, Some("casa.exemplo.br"), None).expect("preparar");
        assert!(
            lancamento.argumentos.is_empty(),
            "abrir sem pedir para hospedar mandou argumento assim mesmo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn cada_recusa_diz_qual_foi() {
        let raiz = pasta("recusas");
        let config = raiz.to_string_lossy().into_owned();
        deposito_com(&raiz, "0.10.5", "bin/seele");

        assert!(matches!(
            preparar(&config, "nao/e/uma/versao", true, None, None),
            Err(FalhaAoAbrirVersao::IdentificadorInvalido)
        ));
        assert!(
            matches!(
                preparar(&config, "9.9.9", true, None, None),
                Err(FalhaAoAbrirVersao::NaoInstalada)
            ),
            "uma versão que não está no disco tem de dizer isso, e não «não \
             iniciei»: o conserto é baixá-la, e a frase é outra"
        );

        // O marcador de antes da anotação do executável: a instalação está boa,
        // e o que falta é a procedência.
        let deposito = Deposito::em(super::raiz_do_deposito(&config));
        let v = Versao::nova("0.10.5").unwrap();
        std::fs::write(deposito.pasta_da_versao(&v).join("INSTALADA"), b"0.10.5\n").unwrap();
        assert!(
            matches!(
                preparar(&config, "0.10.5", true, None, None),
                Err(FalhaAoAbrirVersao::SemProcedencia)
            ),
            "uma instalação sem procedência tem de ser separada de uma ausente: \
             aqui a versão está lá e o conserto é reinstalá-la"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn a_lista_marca_o_que_esta_em_uso_e_o_que_nao_da_para_abrir() {
        let raiz = pasta("lista");
        let config = raiz.to_string_lossy().into_owned();
        deposito_com(&raiz, "0.10.5", "bin/seele");
        deposito_com(&raiz, "0.11.0", "bin/seele");
        // E uma terceira, com marcador antigo.
        deposito_com(&raiz, "0.9.0", "bin/seele");
        let deposito = Deposito::em(super::raiz_do_deposito(&config));
        std::fs::write(
            deposito
                .pasta_da_versao(&Versao::nova("0.9.0").unwrap())
                .join("INSTALADA"),
            b"0.9.0\n",
        )
        .unwrap();

        let lista = instaladas(&config);
        assert_eq!(lista.len(), 3, "o depósito tem três e a lista não as viu");
        let antiga = lista.iter().find(|v| v.versao == "0.9.0").unwrap();
        assert!(
            !antiga.abrivel,
            "a tela vai oferecer um botão que não funciona: uma instalação sem \
             procedência tem de aparecer dita, e não oferecida"
        );
        assert!(
            lista.iter().filter(|v| v.em_uso).count() <= 1,
            "duas versões marcadas como em uso ao mesmo tempo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn abrir_o_que_nao_existe_falha_sem_panico() {
        // `abrir` de verdade, e não só `preparar`: o caminho do `spawn` tem de
        // devolver recusa. Um arquivo que não é executável é o que mais perto se
        // chega, nos três sistemas, de «o sistema recusou iniciar».
        let raiz = pasta("nao-inicia");
        let config = raiz.to_string_lossy().into_owned();
        deposito_com(&raiz, "0.10.5", "bin/seele");
        let resultado = abrir(&config, "0.10.5", false, None, None);
        assert!(
            matches!(resultado, Err(FalhaAoAbrirVersao::NaoIniciou(_))) || resultado.is_ok(),
            "o caminho de abrir devolveu algo que não é nem sucesso nem \
             `NaoIniciou`: {resultado:?}"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
