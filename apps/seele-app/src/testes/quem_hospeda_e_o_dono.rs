use super::a_entrada_velha_que_chega_tarde::{app, runtime, vez};
use super::{config_dir, connect, disconnect, hospedar_em, PedidoDeHospedar, Session};
use seele_proto::control::Permission;
use seele_proto::ids::PersonId;
use tauri::Manager;

#[test]
fn hospedar_declara_a_chave_desta_maquina_dona_e_outra_conta_nao_e() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let config = config_dir(app);
        let anfitriao = hospedar_em(
            &session,
            &config,
            None,
            PedidoDeHospedar::default(),
            0,
            true,
        )
        .await
        .unwrap_or_else(|falha| panic!("o hospedar não subiu: {falha:?}"));

        let entrada = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            connect(
                app.clone(),
                app.state::<Session>(),
                anfitriao.aqui.clone(),
                "dona".to_owned(),
                false,
                None,
                None,
            ),
        )
        .await
        .expect("a entrada de quem hospeda não voltou")
        .unwrap_or_else(|falha| panic!("quem hospeda não entrou no próprio servidor: {falha:?}"));
        let eu = PersonId(entrada.snapshot.me.expect("o servidor disse quem eu sou"));

        let banco = session
            .hospedagens
            .lock()
            .expect("as hospedagens")
            .do_guardado()
            .expect("o guardado de pé")
            .persistence();
        {
            let banco = banco.lock().await;
            let minha = seele_ffi::impressao_desta_maquina(&config).expect("a impressão daqui");
            assert_eq!(
                seele_server::cargos::aviso_sem_dono(&banco),
                None,
                "o servidor que o HOSPEDAR subiu está sem dono"
            );
            for permissao in [
                Permission::ManageVoiceRooms,
                Permission::ManageRoles,
                Permission::AdministerServer,
                Permission::Kick,
            ] {
                assert!(
                    seele_server::cargos::pode(&banco, eu, permissao),
                    "quem hospeda ({minha}) não pode {permissao:?} no próprio servidor"
                );
            }
            assert!(seele_server::cargos::administra(&banco, eu));

            // Outra conta, no mesmo banco: não é dona, nem por ter entrado.
            let outra = seele_server::permissions::Permissions::new(&banco)
                .register_or_find(&[0x77; 32], "outra")
                .expect("a outra conta");
            assert!(
                !seele_server::cargos::pode(&banco, outra.id, Permission::ManageRoles),
                "outra conta virou dona"
            );
            assert!(!seele_server::cargos::administra(&banco, outra.id));
        }

        disconnect(
            app.clone(),
            app.state::<Session>(),
            super::sessoes::CarimboDaJanela::da(&entrada),
        )
        .await
        .expect("o `disconnect` nunca falha");
        let guardado = session
            .hospedagens
            .lock()
            .expect("as hospedagens")
            .guardado
            .take();
        if let Some(guardado) = guardado {
            guardado.hospedagem.encerrar().await;
        }
    });
}
