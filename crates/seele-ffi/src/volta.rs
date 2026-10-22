//! **A volta do enlace**: o buraco do histórico e o que ficou sem resposta.
//! Lote P61 da F5, pendência 61.
//!
//! # O que faltava
//!
//! Medido pela F1 (10/10): numa queda, o que esta máquina mandava no instante
//! ficava em `SemResposta` e esperava alguém apertar TENTAR DE NOVO — o texto da
//! pendência dizia que o cliente repetia sozinho, e o código não fazia. E nada
//! buscava o que passou durante a queda: o núcleo só reassina o último canal, e
//! o histórico só vinha quando a pessoa abria um canal, e só a página mais nova.
//! O que passou durante a queda e não cabia nas 50 mais novas não voltava.
//!
//! # As duas peças, puras
//!
//! [`Buraco`] e [`Repeticao`] não tocam em rede nem em relógio próprio: dizem o
//! que pedir e quando, e o laço da FFI (`drive`) faz. É o que deixa a ordem, o
//! espaçamento e a parada serem provados aqui, por teste de crate, e a prova de
//! ponta a ponta ficar para a conformidade.

use std::time::Instant;

use seele_core::MessageId;

/// **A busca do buraco e a repetição moram no `seele-sessao`** (`volta`) desde
/// a Tarefa 2.6 da F4, genéricas no relógio: o navegador as roda com a duração
/// da página, e aqui elas rodam com o `Instant`. Aqui ficam o laço que as
/// segue ([`seguir`]) e o sono ([`esperar_ate`]).
pub(crate) use seele_core::volta::{grande_demais, repetir, Parada, Passo, Repetir};

/// Quantas mensagens cada página do buraco pede — ver
/// [`seele_core::volta::PAGINA_DO_BURACO`].
pub use seele_core::volta::PAGINA_DO_BURACO;

/// A busca do buraco, no relógio do desktop.
pub(crate) type Buraco = seele_core::volta::Buraco<Instant>;

/// A repetição da volta, no relógio do desktop.
pub(crate) type Repeticao = seele_core::volta::Repeticao<Instant>;

/// Faz o que a busca mandou: pede a página, ou registra a parada.
///
/// Aqui e não no laço da FFI, para os dois braços que acordam a busca — a
/// mensagem que chegou e o relógio dela — fazerem a mesma coisa.
pub(crate) async fn seguir(
    enlace: &seele_core::enlace::Enlace,
    buraco: &mut Option<Buraco>,
    carimbos: &mut u64,
    passo: Passo,
) {
    match passo {
        Passo::Pedir(pagina) => {
            if let Some(por_que) = pagina.de_novo {
                tracing::warn!(
                    canal = pagina.canal.get(),
                    cursor = pagina.cursor.map(MessageId::get),
                    ?por_que,
                    "a página da busca do buraco não teve resposta; pedindo de novo \
                     (o limitador do servidor pode ter descartado o pedido ou a cerca)"
                );
            }
            let _ = enlace
                .historico_com_cerca(pagina.canal, pagina.cursor, pagina.limite, pagina.carimbo)
                .await;
        }
        Passo::Parou(parada) => {
            let Some(parado) = buraco.take() else {
                return;
            };
            *carimbos = parado.sequencia();
            let (canal, paginas, cursor) = (
                parado.canal().get(),
                parado.paginas(),
                parado.cursor().map(MessageId::get),
            );
            match parada {
                Parada::Teto => tracing::warn!(
                    canal,
                    paginas,
                    cursor,
                    "a busca do buraco alcançou o teto de páginas; o que é mais antigo que o \
                     cursor não volta nesta sessão"
                ),
                Parada::SemResposta => tracing::warn!(
                    canal,
                    paginas,
                    cursor,
                    "a busca do buraco desistiu: a cerca da mesma página não voltou duas vezes \
                     no prazo; o que é mais antigo que o cursor não volta nesta sessão"
                ),
                // **O começo do canal sem a última conhecida** (R1-M3 da revisão
                // do P61): ou ela foi apagada durante a queda, ou o servidor
                // mandou páginas menores que `PAGINA_DO_BURACO` e a busca as
                // leu como o começo. O segundo caso é o que a conformidade
                // prende (`a_pagina_da_busca_e_a_pagina_do_servidor`); se um dia
                // acontecer, é aqui que aparece.
                Parada::Comeco => tracing::warn!(
                    canal,
                    paginas,
                    cursor,
                    "a busca do buraco chegou ao começo do canal sem achar a última mensagem que \
                     esta sessão conhecia: ela foi apagada, ou o servidor mandou páginas menores \
                     que as pedidas"
                ),
                Parada::Fechou | Parada::SoAMaisNova => {
                    tracing::info!(canal, paginas, ?parada, "o buraco da volta fechou");
                }
            }
        }
    }
}

/// Dorme até `quando`. Sem ele, nunca volta: é o braço do `select!` de
/// `drive` que nunca é escolhido, como `PortaDaVoz::chegada`.
pub(crate) async fn esperar_ate(quando: Option<Instant>) {
    match quando {
        Some(quando) => tokio::time::sleep_until(quando.into()).await,
        None => std::future::pending().await,
    }
}

/// O que a conferência do tamanho faz nos comandos da FFI. A busca do buraco e
/// a repetição têm os testes delas no `seele-sessao`.
#[cfg(test)]
mod tests {
    use seele_core::state::EstadoDoEnvio;
    use seele_core::ChannelId;

    const CANAL: ChannelId = ChannelId(1);

    /// **TENTAR DE NOVO numa recusada por tamanho não manda nada** (fase D).
    ///
    /// Medido na fase A: `reenviar_mensagem` mandava o corpo de 6 000 bytes ao
    /// enlace, o quadro se recusava a ser codificado, o núcleo chamava `cair()`
    /// e o enlace caía e voltava; a pendente passava de `Recusada(TooLong)` a
    /// `SemResposta`, com a tela oferecendo de novo um reenvio que derruba a
    /// conexão. Agora a conferência é a do envio, `grande_demais`, e a recusa
    /// volta a ser frase.
    #[test]
    fn tentar_de_novo_uma_grande_demais_nao_manda_nada_ao_enlace() {
        let shared = crate::tests::compartilhado_de_teste();
        let (commands, mut fila) = tokio::sync::mpsc::unbounded_channel();
        let conexao = crate::Connection {
            commands,
            shared: std::sync::Arc::clone(&shared),
            encerrada: std::sync::Mutex::new(None),
            ligacao: None,
        };
        let chave = conexao
            .send_message(CANAL.get(), "á".repeat(3_000))
            .unwrap();
        assert!(
            fila.try_recv().is_err(),
            "o envio mandou um corpo grande demais"
        );

        assert_eq!(conexao.reenviar_mensagem(&chave), Ok(true));
        assert!(
            fila.try_recv().is_err(),
            "TENTAR DE NOVO mandou ao enlace um corpo que o derruba"
        );
        let estado = shared
            .room
            .lock()
            .unwrap()
            .pendentes
            .first()
            .map(|pendente| pendente.estado);
        assert_eq!(
            estado,
            Some(EstadoDoEnvio::Falhou(seele_core::MessageRefusal::TooLong {
                limit: 4096
            })),
            "a pendente deixou de dizer por que não sai"
        );
    }

    /// **Uma senha que não cabe no fio é recusada antes de pedir** (R1-M4 da
    /// revisão do P61): nada vai ao enlace, o `Room` não senta a pessoa na sala
    /// pedida — e então não há entrada no ar para desfazer —, e a recusa é a
    /// que o servidor daria a uma senha errada.
    #[test]
    fn uma_senha_grande_demais_nao_pede_a_entrada() {
        let shared = crate::tests::compartilhado_de_teste();
        let (commands, mut fila) = tokio::sync::mpsc::unbounded_channel();
        let conexao = crate::Connection {
            commands,
            shared: std::sync::Arc::clone(&shared),
            encerrada: std::sync::Mutex::new(None),
            ligacao: None,
        };
        let antes = shared.room.lock().unwrap().current_voice_room;

        conexao
            .enter_voice_room(3, Some("á".repeat(17)))
            .expect("a recusa é local, e não erro de conexão");
        assert!(
            fila.try_recv().is_err(),
            "a entrada com a senha grande demais foi pedida ao enlace"
        );
        let room = shared.room.lock().unwrap();
        assert_eq!(room.current_voice_room, antes, "o `Room` sentou a pessoa");
        assert_eq!(
            room.notice.as_ref().map(|aviso| aviso.reason),
            Some(seele_core::AlertReason::VoiceRoomEntryRefused)
        );
        drop(room);

        // No teto, ela sai.
        conexao
            .enter_voice_room(3, Some("x".repeat(seele_core::MAX_NICKNAME_LEN)))
            .unwrap();
        assert!(fila.try_recv().is_ok(), "uma senha no teto não foi pedida");
    }
}
