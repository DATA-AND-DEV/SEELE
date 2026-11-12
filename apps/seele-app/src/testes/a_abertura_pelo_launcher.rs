use super::{atualizacao_permitida, Abertura, FalhaAoAtualizar};

#[test]
fn quem_foi_aberta_pelo_launcher_sabe_disso() {
    assert!(Abertura::de(Vec::<String>::new(), Some("1")).pelo_lancador);
    assert!(!Abertura::de(Vec::<String>::new(), None).pelo_lancador);
    // Só o valor que o launcher põe. Uma variável esquecida no ambiente
    // com outro valor não esconde o botão de ninguém.
    assert!(!Abertura::de(Vec::<String>::new(), Some("0")).pelo_lancador);
}

/// Os argumentos continuam sendo lidos como antes, com a variável junto.
#[test]
fn os_argumentos_do_launcher_continuam_chegando() {
    let aberta = Abertura::de(
        ["--hospedar", "--nome-publico", "casa.exemplo.br"].map(str::to_owned),
        Some("1"),
    );
    assert!(aberta.hospedar && aberta.pelo_lancador);
    assert_eq!(aberta.nome_publico.as_deref(), Some("casa.exemplo.br"));
}

/// Spec 0.16, D-linha-1, valendo também para a 0.16: aberta pelo launcher
/// de uma linha mais nova (uma 2.0 abrindo a 1.x), o atualizador dela
/// trocaria o app do depósito pela versão do topo do manifesto — outra
/// linha. Esconder o botão não basta: o comando recusa.
#[test]
fn aberta_pelo_launcher_o_atualizador_recusa() {
    assert!(matches!(
        atualizacao_permitida(&Abertura::de(Vec::<String>::new(), Some("1"))),
        Err(FalhaAoAtualizar::AbertaPeloLancador)
    ));
    assert!(atualizacao_permitida(&Abertura::de(Vec::<String>::new(), None)).is_ok());
}
