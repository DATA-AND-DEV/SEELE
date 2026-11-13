use super::*;

/// **Uma pendente não pode ser endereçada como mensagem do servidor.**
///
/// R05 abriu a porta e este guarda a fecha: `Message::id` vale zero numa
/// mensagem que o servidor ainda não confirmou, e zero não endereça nada.
///
/// O primeiro caller a pisar nisto foi `conformance::moderacao`, que pegava
/// `messages()[0].id` e recebia «não há tal mensagem» no lugar da recusa de
/// permissão que ele mede. O próximo seria uma tela, e ali o sintoma seria
/// clicar em «remover» numa linha visível e nada acontecer.
#[test]
fn remover_uma_pendente_e_recusado_antes_de_sair() {
    // Um `Connection` com o canal de comandos vivo: o teste não roda laço
    // nenhum, e o que ele mede é a recusa acontecer **antes** de o comando
    // sair. O receptor fica na mão para o canal não fechar.
    let (manda, _recebe) = tokio::sync::mpsc::unbounded_channel();
    let conexao = Connection {
        commands: manda,
        shared: super::tests::compartilhado_de_teste(),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    assert_eq!(
        conexao.remove_message(0),
        Err(ConnectionError::UnknownMessage),
        "o pedido de remover o zero saiu para o servidor: ele responde «não \
             há tal mensagem», que é verdade e é a frase errada para quem clicou \
             num texto que está na tela"
    );
    // E um id de verdade continua saindo: um guarda que barrasse tudo passaria
    // no teste acima e desligaria a remoção inteira.
    assert!(conexao.remove_message(7).is_ok());
}
