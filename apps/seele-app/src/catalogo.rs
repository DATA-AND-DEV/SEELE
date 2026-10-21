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

// **O que decide mora no `seele-sessao`** desde a Tarefa 2.6 da F4: o
// navegador baixa pelo `fetch` e confere, lê, planeja e recusa pelas mesmas
// funções (`seele_ffi::catalogo`). Aqui ficam a chave deste build, a rede e o
// disco.
pub(crate) use seele_ffi::catalogo::{
    conferir_conjunto, planejar, Catalogo, FalhaNoCatalogo, Revogacoes, ENDERECO,
};

/// A chave pública que este build confere, compilada para dentro dele.
///
/// Cópia byte a byte da que o indexador usa para assinar. Ver
/// `apps/seele-app/chaves/LEIA.md` para por que ela é a segunda chave e o que
/// custa trocá-la. O `.wasm` do navegador compila o mesmo arquivo.
const CHAVE_DO_CATALOGO: &str = include_str!("../chaves/mods.pub");

/// Confere a assinatura, contra a chave deste build, e só então analisa.
///
/// # Errors
///
/// As de [`seele_ffi::catalogo::ler_catalogo`].
pub(crate) fn ler_catalogo(bytes: &[u8], assinatura: &str) -> Result<Catalogo, FalhaNoCatalogo> {
    seele_ffi::catalogo::ler_catalogo(bytes, assinatura, CHAVE_DO_CATALOGO)
}

/// O mesmo, para a lista de revogações.
///
/// # Errors
///
/// As de [`seele_ffi::catalogo::ler_revogacoes`].
pub(crate) fn ler_revogacoes(
    bytes: &[u8],
    assinatura: &str,
) -> Result<Revogacoes, FalhaNoCatalogo> {
    seele_ffi::catalogo::ler_revogacoes(bytes, assinatura, CHAVE_DO_CATALOGO)
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
) -> Result<String, FalhaNoCatalogo> {
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
    instalar_os_baixados(std::path::Path::new(config), id, versao, &baixados)
}

/// Instala nesta máquina os arquivos de uma versão já conferida, e devolve o
/// hash do que foi publicado.
///
/// Escritos numa pasta de passagem e instalados pelo caminho de sempre: ele
/// valida o manifesto, recusa atalho e recusa sobrescrever. Um segundo
/// caminho de instalação seria um segundo conjunto de recusas para discordar
/// do primeiro.
///
/// Síncrona e separada da rede para que a recusa local se prove num teste.
///
/// # A recusa daqui é dita como daqui
///
/// Toda recusa desta função é desta máquina: o catálogo já entregou os bytes
/// que declarou. Ela volta como [`FalhaNoCatalogo::NaoInstalou`], com a recusa
/// de instalar dentro, e não como uma falha do catálogo ou da rede; e fica no
/// `seele.log` numa linha com o MOD e a versão. A pasta de passagem vai na
/// linha só como passagem, com `passagem_apagada=` dizendo se ela saiu do
/// disco, o que é conferido e não suposto: como `origem=`, pelo registro de
/// [`crate::mods::instalar_de`], ela parecia a pasta de onde a pessoa
/// instalou, e não dizia qual MOD do catálogo era.
///
/// # Errors
///
/// [`FalhaNoCatalogo::NaoInstalou`], com o que a instalação recusou.
pub(crate) fn instalar_os_baixados(
    config: &std::path::Path,
    id: &str,
    versao: &str,
    baixados: &[(String, Vec<u8>)],
) -> Result<String, FalhaNoCatalogo> {
    let passagem = std::env::temp_dir().join(format!(
        "seele-mod-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let publicado = escrever_e_publicar(config, &passagem, baixados);
    // Conferido, e não afirmado: a linha diz se a passagem saiu do disco.
    let apagada = std::fs::remove_dir_all(&passagem).is_ok();
    // O hash do que foi publicado volta para quem chamou: é por ele que o
    // servidor passa a exigir estes bytes, e não «o que estiver instalado».
    publicado.map(|publicado| publicado.hash).map_err(|recusa| {
        tracing::warn!(
            mod_id = %crate::id_no_registro(id),
            ?versao,
            ?recusa,
            passagem = %crate::caminho_no_registro(&passagem.to_string_lossy()),
            passagem_apagada = apagada,
            "não instalei o MOD do catálogo"
        );
        FalhaNoCatalogo::NaoInstalou(recusa)
    })
}

/// Escreve os arquivos na pasta de passagem e os publica pelo caminho de
/// sempre. Uma falha ao escrever a passagem é do disco daqui, como a da cópia:
/// [`crate::mods::FalhaAoInstalarMod::NaoCopiei`].
fn escrever_e_publicar(
    config: &std::path::Path,
    passagem: &std::path::Path,
    baixados: &[(String, Vec<u8>)],
) -> Result<crate::mods::Publicado, crate::mods::FalhaAoInstalarMod> {
    use crate::mods::FalhaAoInstalarMod;
    let no_disco = |erro: std::io::Error| FalhaAoInstalarMod::NaoCopiei(erro.to_string());
    for (relativo, bytes) in baixados {
        let destino = passagem.join(relativo);
        if let Some(pai) = destino.parent() {
            std::fs::create_dir_all(pai).map_err(no_disco)?;
        }
        std::fs::write(&destino, bytes).map_err(no_disco)?;
    }
    crate::mods::publicar_de(config, passagem)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod o_catalogo {
    use super::{conferir_conjunto, ler_catalogo, planejar, Catalogo, FalhaNoCatalogo, Revogacoes};
    use crate::mods::FalhaAoInstalarMod;

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
        // **Duas versões, e a ordem importa.** O catálogo publica da mais antiga
        // para a mais nova, e a tela lê a última para oferecer. Com uma versão
        // só isto não distinguia nada; com duas, distingue.
        assert!(
            mesa.versoes.len() >= 2,
            "o vetor voltou a ter uma versão só, e a ordem deixa de ser exercitada"
        );
        let primeira = mesa
            .versoes
            .first()
            .expect("um MOD no catálogo sem versão nenhuma não é instalável");
        let ultima = mesa.versoes.last().expect("a lista não está vazia");

        // **A propriedade, e não os números.** A versão aqui era `1.2.0` e
        // `1.2.1` cravadas; o vetor foi trocado quando a API 3 saiu, e o teste
        // reprovou dizendo `2.0.0 != 1.2.1` — que fala do vetor e não de nada
        // que este build faça errado.
        //
        // Cravar o número faz o teste reprovar toda vez que o indexador
        // publica, e um teste que reprova por motivo certo na hora errada é um
        // teste que ensina a ignorá-lo. O que importa é que a lista esteja em
        // ordem crescente, porque a tela **lê a última para oferecer**.
        let numero = |v: &str| -> Vec<u32> {
            v.split(['.', '-'])
                .map(|p| p.parse::<u32>().unwrap_or(0))
                .collect()
        };
        assert!(
            numero(&primeira.versao) < numero(&ultima.versao),
            "o catálogo deixou de publicar da mais antiga para a mais nova: \
             {} vem antes de {}",
            primeira.versao,
            ultima.versao
        );
        for (antes, depois) in mesa.versoes.iter().zip(mesa.versoes.iter().skip(1)) {
            assert!(
                numero(&antes.versao) < numero(&depois.versao),
                "as versões saíram de ordem entre {} e {}, e a tela oferece a \
                 última da lista",
                antes.versao,
                depois.versao
            );
        }
        assert_ne!(
            primeira.hash, ultima.hash,
            "duas versões com o mesmo conteúdo são uma publicação errada"
        );
        for v in &mesa.versoes {
            assert_eq!(v.hash.len(), 64, "o hash do conteúdo é um SHA-256");
            assert!(
                !v.arquivos.is_empty(),
                "uma versão sem arquivos não instala"
            );
        }
        // Que a API publicada caiba nesta build é conferido em
        // `seele-conformance`, e não aqui: esta casca não depende do
        // `seele-proto` (ADR 0039), e copiar `MOD_API_VERSION` para dentro dela
        // seria a terceira cópia do número cuja divergência custou a primeira
        // publicação.
        assert!(
            mesa.versoes.iter().all(|v| v.api > 0),
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

    /// Uma pasta só deste teste, fora de qualquer `config` de verdade.
    fn pasta_de_teste(nome: &str) -> std::path::PathBuf {
        let caminho = std::env::temp_dir().join(format!(
            "seele-catalogo-{}-{}-{nome}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&caminho).unwrap();
        caminho
    }

    /// Os arquivos de uma versão como chegam do catálogo, já conferidos, com o
    /// `mod.json` declarando a API `api`.
    fn baixados(api: u32) -> Vec<(String, Vec<u8>)> {
        let manifesto = format!(
            r#"{{"schema":1,"id":"alguem/rpg","version":"1.0.0","api":{api},
                "repo":"https://example.invalid/rpg","reach":["dom"],
                "client":"cliente/main.js"}}"#
        );
        vec![
            ("mod.json".to_owned(), manifesto.into_bytes()),
            ("cliente/main.js".to_owned(), b"globalThis.x = 1;".to_vec()),
        ]
    }

    /// A recusa é a variante própria, com a recusa de instalar dentro, e a
    /// linha WARN diz o MOD e a versão; a pasta de passagem só aparece como
    /// passagem.
    fn exige_a_recusa_local(
        resultado: &Result<String, FalhaNoCatalogo>,
        rastro: &str,
        esperada: impl Fn(&FalhaAoInstalarMod) -> bool,
    ) {
        assert!(
            matches!(resultado, Err(FalhaNoCatalogo::NaoInstalou(recusa)) if esperada(recusa)),
            "a recusa desta máquina não chegou como `NaoInstalou` com a recusa de instalar \
             dentro, e a tela culpa o catálogo ou a rede: {resultado:?}"
        );
        assert!(
            rastro.lines().any(|linha| linha.contains("WARN")
                && linha.contains("mod_id=alguem/rpg")
                && linha.contains(r#"versao="1.0.0""#)
                && linha.contains("recusa=")),
            "a recusa local de um MOD do catálogo não deixou linha WARN com o MOD, a versão e a \
             recusa, e quem relata «não instala» chega sem dado nenhum:\n{rastro}"
        );
        assert!(
            !rastro.contains("origem="),
            "a pasta de passagem, apagada logo depois, aparece no `seele.log` como a origem do \
             MOD:\n{rastro}"
        );
        // **A passagem que a linha nomeia já não existe** (rodada 1, M1), em todo
        // caminho de falha, e a linha diz que conferiu. A pasta vai no registro
        // pelo `Debug`, que dobra a barra invertida do Windows.
        let passagem = rastro
            .split("passagem=\"")
            .nth(1)
            .and_then(|resto| resto.split('"').next())
            .expect("a linha WARN não nomeia a pasta de passagem")
            .replace(r"\\", r"\");
        let passagem = std::path::Path::new(&passagem);
        assert_eq!(
            passagem.parent(),
            Some(std::env::temp_dir().as_path()),
            "a pasta lida da linha não é uma passagem em `temp_dir()`, e conferir que ela não \
             existe não provaria nada: {}",
            passagem.display()
        );
        assert!(
            !passagem.exists(),
            "a pasta de passagem ficou em `temp_dir()` depois da recusa: {}",
            passagem.display()
        );
        assert!(
            rastro.contains("passagem_apagada=true"),
            "a linha WARN não diz que a pasta de passagem foi apagada:\n{rastro}"
        );
    }

    /// **A recusa local chega à janela na forma que o `frases.js` lê** (rodada
    /// 1, M3). A bancada `hospedar-com-mod-antigo.cjs` monta as quatro à mão,
    /// «como o serde as manda»: um `#[serde(rename)]` ou um `tag` posto na enum
    /// deixaria a bancada verde e a tela em «ALGO FALHOU».
    #[test]
    fn a_recusa_local_chega_a_janela_na_forma_que_a_frase_le() {
        use serde_json::json;
        let forma = |recusa| serde_json::to_value(FalhaNoCatalogo::NaoInstalou(recusa)).unwrap();
        for (recusa, esperada) in [
            (
                FalhaAoInstalarMod::NaoCopiei("x".to_owned()),
                json!({"NaoInstalou": {"NaoCopiei": "x"}}),
            ),
            (
                FalhaAoInstalarMod::ManifestoRecusado("api-too-new".to_owned()),
                json!({"NaoInstalou": {"ManifestoRecusado": "api-too-new"}}),
            ),
            (
                FalhaAoInstalarMod::TemAtalho("/x".to_owned()),
                json!({"NaoInstalou": {"TemAtalho": "/x"}}),
            ),
            (
                FalhaAoInstalarMod::SemManifesto,
                json!({"NaoInstalou": "SemManifesto"}),
            ),
        ] {
            assert_eq!(
                forma(recusa),
                esperada,
                "a recusa local chega à janela numa forma que o `fraseDeErro` não lê"
            );
        }
    }

    /// **A recusa local de um MOD do catálogo é dita como a recusa de instalar,
    /// e fica no `seele.log` com o MOD e a versão** (pendência 58, item 9).
    ///
    /// O catálogo entregou os bytes certos, e quem recusou foi esta máquina.
    /// Antes, o `mod.json` recusado chegava à tela como `NaoEUmCatalogo` («O
    /// CATÁLOGO VEIO QUEBRADO… Avise quem publica»), e a linha do registro
    /// apontava a pasta de passagem, já apagada, sem dizer qual MOD era.
    #[test]
    fn o_mod_json_recusado_de_um_mod_do_catalogo_nao_culpa_o_catalogo() {
        let raiz = pasta_de_teste("recusa-local");
        let config = raiz.join("config");

        // O `mod.json` que este SEELE recusa: uma API que ele não roda.
        let (resultado, rastro) = crate::rastro_de_teste::capturar(|| {
            super::instalar_os_baixados(&config, "alguem/rpg", "1.0.0", &baixados(99))
        });
        let _ = std::fs::remove_dir_all(&raiz);

        exige_a_recusa_local(&resultado, &rastro, |recusa| {
            let FalhaAoInstalarMod::ManifestoRecusado(motivo) = recusa else {
                return false;
            };
            motivo == "api-too-new"
        });
    }

    /// **A pasta de passagem que não se escreve é falha do disco daqui**, e não
    /// da rede: antes ela chegava como `NaoRespondeu` («Pode ser a sua rede»).
    /// A falha é injetada com um arquivo e uma pasta do mesmo nome.
    #[test]
    fn a_passagem_que_nao_se_escreve_nao_culpa_a_rede() {
        let raiz = pasta_de_teste("sem-passagem");
        let config = raiz.join("config");
        let mut sem_passagem = baixados(seele_ffi::mods::MOD_API_VERSION);
        sem_passagem.push(("cliente".to_owned(), b"sou um arquivo".to_vec()));

        let (resultado, rastro) = crate::rastro_de_teste::capturar(|| {
            super::instalar_os_baixados(&config, "alguem/rpg", "1.0.0", &sem_passagem)
        });
        let _ = std::fs::remove_dir_all(&raiz);

        exige_a_recusa_local(&resultado, &rastro, |recusa| {
            matches!(recusa, FalhaAoInstalarMod::NaoCopiei(_))
        });
    }

    /// **E o disco de `config` que não deixa copiar**, a recusa local mais
    /// provável: disco cheio, ou sem permissão.
    // **Só onde há permissão de Unix.** O Windows não tem `PermissionsExt`.
    #[cfg(unix)]
    #[test]
    fn o_disco_que_nao_deixa_instalar_um_mod_do_catalogo_nao_culpa_o_catalogo() {
        use std::os::unix::fs::PermissionsExt as _;

        let raiz = pasta_de_teste("sem-permissao");
        let config = raiz.join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o500)).unwrap();
        let tranca_vale = std::fs::write(config.join("teste"), b"x").is_err();

        let (resultado, rastro) = crate::rastro_de_teste::capturar(|| {
            super::instalar_os_baixados(
                &config,
                "alguem/rpg",
                "1.0.0",
                &baixados(seele_ffi::mods::MOD_API_VERSION),
            )
        });
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o700)).unwrap();
        let _ = std::fs::remove_dir_all(&raiz);

        assert!(
            tranca_vale,
            "a pasta sem permissão de escrita ainda se escreve (este teste roda como root?), e \
             sem a falha injetada a instalação termina e o teste não prova nada"
        );
        exige_a_recusa_local(&resultado, &rastro, |recusa| {
            matches!(recusa, FalhaAoInstalarMod::NaoCopiei(_))
        });
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

    /// **O defeito que não aparece na máquina de quem o introduz.**
    ///
    /// Os vetores desta suíte são conferidos por assinatura, e uma assinatura
    /// vale sobre **bytes**. O Git do Windows converte LF em CRLF na cópia de
    /// trabalho por padrão: o catálogo chega ao disco com 226 bytes de `\r` a
    /// mais do que o arquivo que foi assinado, e a conferência recusa — como
    /// tem de recusar. Foi assim que a bateria da v0.11.0 passou no Mac e
    /// reprovou no Windows, nestes dois testes e em nenhum outro:
    ///
    /// ```text
    /// o_catalogo_que_o_indexador_gera_e_aceito_por_este_build
    /// as_revogacoes_que_o_indexador_gera_tambem_sao_aceitas
    /// ```
    ///
    /// E a mensagem falava de assinatura, que é o sintoma. A causa estava no
    /// checkout, antes de qualquer código rodar.
    ///
    /// O conserto é o `.gitattributes` da raiz. Este guarda existe porque o
    /// conserto é **invisível**: quem acrescentar o próximo vetor assinado não
    /// tem como descobrir sozinho que precisa declará-lo, porque a máquina dele
    /// vai passar. Aqui ele descobre na hora, em qualquer sistema.
    #[test]
    fn todo_vetor_cujos_bytes_sao_o_contrato_fica_fora_do_conversor_de_fim_de_linha() {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("o `apps/seele-app` mora dois níveis abaixo da raiz")
            .to_path_buf();
        let regras = std::fs::read_to_string(raiz.join(".gitattributes")).expect(
            "o `.gitattributes` da raiz é o que mantém os vetores assinados \
             intactos no Windows; sem ele a bateria reprova lá e passa aqui",
        );

        // As linhas que tiram o arquivo do conversor. O que interessa de cada
        // uma é o padrão — a primeira palavra.
        let sem_conversao: Vec<&str> = regras
            .lines()
            .map(str::trim)
            .filter(|linha| !linha.starts_with('#') && !linha.is_empty())
            .filter(|linha| linha.split_whitespace().any(|campo| campo == "-text"))
            .filter_map(|linha| linha.split_whitespace().next())
            .collect();
        assert!(
            !sem_conversao.is_empty(),
            "o `.gitattributes` existe mas não tira nada do conversor"
        );

        // Onde moram os arquivos cujos bytes são conferidos. A prosa que os
        // explica (`.md`) não é conferida por ninguém, e pode ser convertida à
        // vontade.
        // A regra que vale para o repositório inteiro. Sem ela o `\r` alcança
        // `empacotar/publicar.sh` — `morrer() {` deixa de ser shell válido — e
        // 44 testes do `xtask` caem junto, atrás destes dois.
        assert!(
            regras
                .lines()
                .map(str::trim)
                .any(|linha| linha.starts_with("* ")
                    && linha.contains("text=auto")
                    && linha.contains("eol=lf")),
            "sumiu a regra `* text=auto eol=lf`. É ela que faz a cópia de \
             trabalho sair com LF em todo sistema; sem ela o Windows converte \
             os scripts, e o orquestrador de release para de ser shell válido"
        );

        // As versões da API de MODs. A regra de cima já as tira do CRLF no
        // checkout; o que esta linha guarda é o commit de quem as escreve no
        // Windows (ver o comentário dela no `.gitattributes`).
        assert!(
            sem_conversao.contains(&"api/*.json"),
            "sumiu a linha `api/*.json -text` do `.gitattributes`. Sem ela, uma `api/vN.json` \
             escrita com CRLF é normalizada para LF no commit, mas a cópia de trabalho de quem \
             a escreveu continua com CRLF: o hash que `a_api_publicada_nao_se_edita` lhe dá, e \
             que vai para `api/congeladas.sha256`, não é o que o CI vê — verde na máquina \
             dele, vermelho nos três sistemas"
        );

        let pastas = [
            "apps/seele-app/testes",
            "apps/seele-app/chaves",
            "crates/seele-lancador/testes",
        ];
        let mut desprotegidos = Vec::new();

        // Um vetor solto na raiz, que este repositório e o indexador têm de ler
        // byte a byte igual.
        if !sem_conversao.contains(&"vetores-de-hash.json") {
            desprotegidos.push("vetores-de-hash.json".to_owned());
        }
        for pasta in pastas {
            let leitura = std::fs::read_dir(raiz.join(pasta))
                .unwrap_or_else(|erro| panic!("não consegui ler `{pasta}`: {erro}"));
            for item in leitura {
                let caminho = item.expect("entrada de diretório").path();
                if !caminho.is_file() {
                    continue;
                }
                let nome = caminho
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .expect("nome de arquivo em UTF-8")
                    .to_owned();
                if nome.ends_with(".md") {
                    continue;
                }
                let relativo = format!("{pasta}/{nome}");
                // Duas formas de declarar, e as duas contam: o caminho inteiro,
                // ou uma extensão que o alcance.
                let coberto = sem_conversao.iter().any(|padrao| {
                    *padrao == relativo
                        || padrao
                            .strip_prefix('*')
                            .is_some_and(|sufixo| nome.ends_with(sufixo))
                });
                if !coberto {
                    desprotegidos.push(relativo);
                }
            }
        }
        assert!(
            desprotegidos.is_empty(),
            "estes arquivos são conferidos byte a byte e o Git do Windows vai \
             convertê-los no checkout — declare-os com `-text` no \
             `.gitattributes` da raiz: {desprotegidos:?}"
        );

        // E a outra metade: se um deles já chegou convertido, a mensagem tem de
        // dizer isso, e não deixar a suíte falar de assinatura.
        for (nome, bytes) in [
            (
                "apps/seele-app/testes/catalogo-do-indexador.json",
                &include_bytes!("../testes/catalogo-do-indexador.json")[..],
            ),
            (
                "apps/seele-app/testes/revogacoes-do-indexador.json",
                &include_bytes!("../testes/revogacoes-do-indexador.json")[..],
            ),
            (
                "apps/seele-app/chaves/mods.pub",
                super::CHAVE_DO_CATALOGO.as_bytes(),
            ),
        ] {
            assert!(
                !bytes.contains(&b'\r'),
                "`{nome}` chegou com CRLF. Não é a assinatura que está errada: \
                 é o checkout. Confira o `.gitattributes` da raiz e refaça o \
                 clone, ou rode `git add --renormalize .`"
            );
        }
    }
}
