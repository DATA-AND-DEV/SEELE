use super::a_entrada_velha_que_chega_tarde::{app, runtime, servidor, vez};
use super::{
    connect, disconnect, onde_estou_na, parar_de_hospedar_em, servidor_do_mod, Hospedado,
    OndeEstou, Session,
};
use seele_server::hospedagem::Hospedagem;
use seele_server::mods::instalar::{instalar_de_pasta, raizes_de_um_mundo};
use seele_server::permissions::Permissions;
use seele_server::persistence::mods::{enable, EnabledMod};
use seele_server::persistence::{Location, Persistence};
use std::time::Duration;
use tauri::test::MockRuntime;
use tauri::{AppHandle, Manager};

/// O MOD de referência do produto, que tem as duas metades.
const MOD_DE_REFERENCIA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testes/mod-de-referencia");

/// Entra em `alvo` pelo `connect` da casca, até 30 s cada tentativa. Um
/// servidor com MOD pergunta antes (`ModsNaoAceitos`): a dona aceita como a
/// casca aceita (o corpo de `aceitar_mods`) e entra de novo.
async fn entrar(app: &AppHandle<MockRuntime>, alvo: &str) {
    for _ in 0..2 {
        let tentativa = tokio::time::timeout(
            Duration::from_secs(30),
            connect(
                app.clone(),
                app.state::<Session>(),
                alvo.to_owned(),
                "dona".to_owned(),
                false,
                None,
                None,
            ),
        )
        .await
        .unwrap_or_else(|_| panic!("o `connect` a {alvo} não voltou"));
        let Err(falha) = tentativa else {
            return;
        };
        let dita = serde_json::to_value(&falha).unwrap_or_default();
        let Some(conjunto) = dita
            .pointer("/error/ModsNaoAceitos/conjunto")
            .and_then(serde_json::Value::as_str)
        else {
            panic!("o `connect` a {alvo} não entrou: {dita}");
        };
        seele_ffi::mods::aceitar(&super::config_dir(app), alvo, conjunto)
            .expect("o sim aos MODs não gravou");
    }
    panic!("o `connect` a {alvo} não entrou nem com o sim aos MODs");
}

/// O que o MOD `id` responde a `contar`, pedido por quem ficou no
/// guardado, no servidor que os comandos de MOD do app acham. `quando` diz
/// onde a dona está, para a falha dizer em que ponto o guardado sumiu.
async fn contar(session: &tauri::State<'_, Session>, id: &str, quando: &str) -> serde_json::Value {
    let servidor = servidor_do_mod(session).unwrap_or_else(|falha| {
        panic!("{quando}, os comandos de MOD não acham o guardado: {falha:?}")
    });
    // Quem ficou: uma conta do guardado, criada depois da dona, que é a
    // primeira e por isso a Comandante.
    let quem_ficou = {
        let banco = servidor.persistence.lock().await;
        Permissions::new(&banco)
            .register_or_find(&[7; 32], "quem-ficou")
            .expect("a conta de quem ficou")
            .id
    };
    let resposta = seele_server::mods::pedidos::executar(
        &servidor,
        quem_ficou,
        // O canal zero, «nenhum canal»: por `into`, porque a casca não
        // nomeia o `seele-proto` (ADR 0002).
        0_u32.into(),
        id,
        r#"{"op":"contar"}"#,
    )
    .await;
    serde_json::from_str(&resposta).unwrap_or_else(|_| serde_json::json!({ "resposta": resposta }))
}

#[test]
fn um_mod_do_guardado_continua_respondendo_enquanto_o_dono_visita_outro_servidor() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();

        // O guardado, com o MOD de referência ligado antes de subir: a
        // subida carrega o que está ligado.
        let mundo = tempfile::tempdir().expect("a pasta do guardado não abriu");
        let raizes = raizes_de_um_mundo(mundo.path());
        let (manifesto, hash) = instalar_de_pasta(std::path::Path::new(MOD_DE_REFERENCIA), &raizes)
            .expect("o MOD de referência não instalou");
        let banco = Location::File(mundo.path().join("seele.db"));
        enable(
            &Persistence::open(&banco).expect("o banco do guardado não abriu"),
            &EnabledMod {
                id: manifesto.id.clone(),
                version: manifesto.version.clone(),
                hash,
                repo: manifesto.repo.clone(),
                reach: manifesto.reach.clone(),
                server_half: manifesto.server.is_some(),
                api: manifesto.api,
                tema: manifesto.tema,
                plataformas: None,
            },
        )
        .expect("o MOD de referência não ligou");
        let guardado = Hospedagem::iniciar(0, banco, "Casa", Some(raizes), |_| Ok(()))
            .await
            .expect("o guardado não subiu");
        let casa = format!("127.0.0.1:{}", guardado.endereco().port());
        session.hospedagens.lock().expect("as hospedagens").guardado =
            Some(Hospedado::novo(guardado, None));

        // A dona entra no próprio servidor, e o MOD responde.
        entrar(app, &casa).await;
        assert_eq!(
            onde_estou_na(&session),
            OndeEstou::Guardado,
            "a dona não entrou no próprio servidor, e o teste não mede a visita"
        );
        let dentro = contar(&session, &manifesto.id, "com a dona dentro").await;
        assert_eq!(
            dentro.get("vezes"),
            Some(&serde_json::json!(1)),
            "o MOD do guardado não responde nem com a dona dentro, e o teste não mede a \
                 visita: {dentro}"
        );

        // A dona sai e vai a outro servidor.
        disconnect(
            app.clone(),
            app.state::<Session>(),
            session.carimbo_da_vista(),
        )
        .await
        .expect("o `disconnect` nunca falha");
        let (outro, servidor_outro) = servidor("Outro").await;
        entrar(app, &outro.to_string()).await;
        assert_eq!(
            onde_estou_na(&session),
            OndeEstou::Outro,
            "a dona não está noutro servidor, e o teste não mede a visita"
        );

        // E o MOD do guardado continua respondendo a quem ficou: a mesma
        // instância, com a conta de antes.
        let fora = contar(&session, &manifesto.id, "com a dona noutro servidor").await;
        assert_eq!(
            fora.get("vezes"),
            Some(&serde_json::json!(2)),
            "o MOD do guardado não continuou respondendo a quem ficou nele, com a dona \
                 noutro servidor: {fora}"
        );

        disconnect(
            app.clone(),
            app.state::<Session>(),
            session.carimbo_da_vista(),
        )
        .await
        .expect("o `disconnect` nunca falha");
        assert!(
            parar_de_hospedar_em(&session).await,
            "o guardado não estava mais no ar no fim"
        );
        servidor_outro.shutdown();
    });
}
