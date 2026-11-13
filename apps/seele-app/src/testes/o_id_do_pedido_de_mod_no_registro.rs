use super::{pedido_a_caminho_dito, pedido_recusado_na_saida_dito, ConnectionError};
use crate::rastro_de_teste::capturar;

/// As duas linhas de `mod_request`, cada uma com o rastro dela.
fn as_duas_linhas(id: &str) -> [(&'static str, String); 2] {
    let ((), a_caminho) = capturar(|| pedido_a_caminho_dito(id, 7, 3, 1));
    let ((), recusado) = capturar(|| {
        pedido_recusado_na_saida_dito(id, 3, &ConnectionError::NotConnected);
    });
    [
        ("pedido_a_caminho_dito", a_caminho),
        ("pedido_recusado_na_saida_dito", recusado),
    ]
}

#[test]
fn uma_quebra_de_linha_no_id_nao_forja_outra_linha() {
    for (linha, rastro) in as_duas_linhas("a/b\nWARN seele_app: forjada") {
        assert_eq!(
            rastro.lines().count(),
            1,
            "o id que a janela manda escreveu, por `{linha}`, uma segunda linha no seele.log \
                 com a cara do produto: {rastro}"
        );
        assert!(
            rastro.contains("mod_id=a/bWARN seele_app: forjada"),
            "`{linha}` perdeu o id, ou o escreveu de outro jeito que as recusas \
                 (`id_no_registro`): {rastro}"
        );
    }
}

/// **O id vazio é o catálogo**, como na metade servidor (`executar`, em
/// `seele-server/src/mods/pedidos.rs`): `catalogo=true`, e nenhum
/// `mod_id=` em branco que uma busca por `mod_id=` ache.
#[test]
fn o_id_vazio_e_o_catalogo_e_nao_um_mod_id_em_branco() {
    for (linha, rastro) in as_duas_linhas("") {
        assert!(
            rastro.contains("catalogo=true") && !rastro.contains("mod_id"),
            "`{linha}` escreveu o pedido do catálogo, que não é de MOD nenhum, sem \
                 `catalogo=true` ou com um `mod_id=` em branco: {rastro}"
        );
    }
}

/// **Um id que o filtro esvazia é ilegível**, e não um `mod_id=` em
/// branco: o terceiro braço da metade servidor.
#[test]
fn um_id_que_o_filtro_esvazia_e_ilegivel() {
    for (linha, rastro) in as_duas_linhas("\n\u{202E} ") {
        assert!(
            rastro.contains("mod_id_ilegivel=true") && !rastro.contains("mod_id="),
            "`{linha}` escreveu um id que o filtro esvazia como um `mod_id=` em branco, e \
                 não como ilegível: {rastro}"
        );
    }
}

/// **A mídia vinda do servidor também**: a linha saiu de `midia_em_bytes`
/// para uma função medida, e o id que a janela manda continua passando por
/// `id_no_registro`.
#[test]
fn a_midia_vinda_do_servidor_nao_forja_linha() {
    let ((), rastro) = capturar(|| {
        super::midia_vinda_do_servidor_dita(
            &super::BaldesDoRegistro::default(),
            crate::relogio_de_teste::origem(),
            "a/b\nWARN seele_app: forjada",
            7,
            "imagem",
            8,
        );
    });
    assert!(
        rastro.lines().count() == 1 && rastro.contains("mod_id=a/bWARN seele_app: forjada"),
        "o id que a janela manda escreveu, pela mídia vinda do servidor, uma segunda linha \
             no seele.log, ou saiu de outro jeito que as recusas: {rastro}"
    );
}

#[test]
fn um_id_de_verdade_sai_como_mod_id() {
    for (linha, rastro) in as_duas_linhas("seele/perfis") {
        assert!(
            rastro
                .split_whitespace()
                .any(|pedaco| pedaco == "mod_id=seele/perfis"),
            "`{linha}` deixou de escrever `mod_id=seele/perfis`, que é o que o guia manda \
                 procurar: {rastro}"
        );
    }
}

/// O corpo de `funcao` neste arquivo, sem as linhas de comentário
/// ([`super::fonte_sem_comentarios`]) e sem espaço nenhum.
fn corpo_sem_espaco(fonte: &str, funcao: &str) -> String {
    super::corpo_de(fonte, funcao)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// **Nenhum `mod_id = %id` usa o id cru do comando**, nas três portas que
/// o escreviam assim e nas duas funções de `mod_request`. Os comandos não
/// sobem num teste — precisam de conexão, de `AppHandle` ou do seletor do
/// sistema —, e uma linha escrita à mão dentro de um deles deixaria os
/// testes acima verdes com o id cru no registro de novo.
#[test]
fn nenhum_mod_id_em_display_usa_o_id_cru_do_comando() {
    let fonte = super::fonte_sem_comentarios();
    // A função e o nome que o id cru tem nela.
    for (funcao, cru) in [
        ("fn mod_request(", "id"),
        ("fn pedido_a_caminho_dito(", "id"),
        ("fn pedido_recusado_na_saida_dito(", "id"),
        ("fn midia_em_bytes(", "id"),
        ("fn midia_vinda_do_servidor_dita(", "mod_id"),
        ("async fn escolher_para_o_mod(", "id"),
    ] {
        let codigo = corpo_sem_espaco(&fonte, funcao);
        for depois in codigo.split("mod_id=%").skip(1) {
            let valor: String = depois
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            assert_ne!(
                valor, cru,
                "`{funcao}` escreve `mod_id = %{cru}` com o id cru que a janela manda: em \
                     `%` ele sai sem escape, e uma quebra de linha nele escreve uma segunda \
                     linha no seele.log com a cara do produto"
            );
        }
    }
    let pedido = corpo_sem_espaco(&fonte, "fn mod_request(");
    for funcao in ["pedido_a_caminho_dito(", "pedido_recusado_na_saida_dito("] {
        assert!(
            pedido.contains(funcao),
            "`fn mod_request(` deixou de passar por `{funcao}`, e a linha escrita à mão no \
                 comando escapa dos testes acima"
        );
    }
}
