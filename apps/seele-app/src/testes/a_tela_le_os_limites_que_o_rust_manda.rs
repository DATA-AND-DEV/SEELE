use super::RegrasDoIcone;

/// **A02 da auditoria de 17/09.** A tela de preparar mostrava, literalmente,
/// `Até NaN KiB, undefined px de lado`.
///
/// Eu li `regras.teto_em_bytes` e `regras.lado_maximo`; o Rust serializa
/// `limite_bytes` e `lado`. A tela de CONFIGURAÇÕES já usava os certos —
/// dois leitores do mesmo comando, e só um deles conferiu o nome.
///
/// O guarda deriva as chaves de um `RegrasDoIcone` **serializado de
/// verdade**, e não de uma lista escrita aqui: assim ele não pode divergir
/// do Rust no dia em que o campo mudar de nome.
///
/// A forma cobrada é a mesma do guarda dos MODs: ler de um objeto do Rust
/// um nome que ele não produz não lança, não aparece no console e não
/// reprova teste nenhum. Chega na tela como `undefined`.
#[test]
fn nenhuma_tela_le_um_limite_de_imagem_que_nao_existe() {
    let Ok(serde_json::Value::Object(mapa)) = serde_json::to_value(RegrasDoIcone {
        limite_bytes: 8 * 1024,
        lado: 256,
    }) else {
        panic!("`RegrasDoIcone` tem de serializar para objeto");
    };
    let chaves: std::collections::BTreeSet<&str> = mapa.keys().map(String::as_str).collect();
    // **E a regra da senha do servidor** (1.1.1), pela mesma razão e do mesmo
    // jeito: as chaves de um `senha_do_servidor::Regras` serializado de
    // verdade. Quem a lê, na tela de preparar, tem `regrasDaSenha` no nome.
    let Ok(serde_json::Value::Object(da_senha)) =
        serde_json::to_value(crate::senha_do_servidor::regras())
    else {
        panic!("`senha_do_servidor::Regras` tem de serializar para objeto");
    };
    let chaves_da_senha: std::collections::BTreeSet<&str> =
        da_senha.keys().map(String::as_str).collect();

    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut culpados = Vec::new();
    for arquivo in ["tela-boot.js", "tela-server.js"] {
        let script = std::fs::read_to_string(raiz.join(arquivo))
            .unwrap_or_else(|erro| panic!("{arquivo} tem de ser legível: {erro}"));
        // Toda leitura de `<algo com "regras" no nome>.<campo>`.
        for (at, _) in script.match_indices("regras") {
            let Some(resto) = script.get(at..) else {
                continue;
            };
            let Some(ponto) = resto.find('.') else {
                continue;
            };
            // Só o acesso imediato: `regras.x`, `regrasDoIcone.x`.
            if resto[..ponto].contains(['(', ')', ' ', '\n', ';']) {
                continue;
            }
            let campo: String = resto[ponto + 1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let validas = if resto.starts_with("regrasDaSenha") {
                &chaves_da_senha
            } else {
                &chaves
            };
            if !campo.is_empty() && !validas.contains(campo.as_str()) {
                culpados.push(format!("{arquivo}: {}.{campo}", &resto[..ponto]));
            }
        }
    }

    assert!(
        culpados.is_empty(),
        "estas telas leem um limite que o Rust não serializa, e o que chega é `undefined`: \
         {culpados:?}. As chaves que existem são {chaves:?} (a imagem) e {chaves_da_senha:?} \
         (a senha do servidor, lida por um nome que começa com `regrasDaSenha`)"
    );
}

/// Os tetos que cada tela lê de `limites_dos_campos`, por arquivo: os nomes
/// passados a `passaDoTeto("…"` e os lidos direto de `tetosDosCampos())?.…`.
fn tetos_lidos_pelas_telas() -> Vec<(String, String)> {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut lidos = Vec::new();
    let mut arquivos: Vec<_> = std::fs::read_dir(&raiz)
        .expect("ui/ tem de ser legível")
        .filter_map(Result::ok)
        .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
        .filter(|nome| nome.ends_with(".js"))
        .collect();
    arquivos.sort();
    for arquivo in arquivos {
        let script = std::fs::read_to_string(raiz.join(&arquivo))
            .unwrap_or_else(|erro| panic!("{arquivo} tem de ser legível: {erro}"));
        for pedaco in script.split("passaDoTeto(\"").skip(1) {
            if let Some(campo) = pedaco.split('"').next() {
                lidos.push((arquivo.clone(), campo.to_owned()));
            }
        }
        for pedaco in script.split("tetosDosCampos())?.").skip(1) {
            // `?.[campo]` é o acesso por nome variável do próprio
            // `passaDoTeto`, cujos nomes são os de quem o chama, lidos acima.
            if pedaco.starts_with('[') {
                continue;
            }
            let campo: String = pedaco
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            lidos.push((arquivo.clone(), campo));
        }
    }
    lidos
}

/// **E os tetos dos campos, pela mesma razão** (pendência 58, item 55; lote
/// C55 da F5).
///
/// A janela pede cada teto pelo nome, e um nome que o Rust não serializa
/// não lança: chega `undefined`, `passaDoTeto` não confere nada, e o campo
/// volta a mandar calado o que o núcleo recusa. As chaves vêm de um
/// `LimitesDosCampos` serializado de verdade. E, na outra mão, todo teto
/// que o Rust serializa tem uma tela que o lê.
#[test]
fn nenhuma_tela_le_um_teto_de_campo_que_nao_existe() {
    let Ok(serde_json::Value::Object(mapa)) =
        serde_json::to_value(seele_ffi::Connection::limites_dos_campos())
    else {
        panic!("`LimitesDosCampos` tem de serializar para objeto");
    };
    let chaves: std::collections::BTreeSet<&str> = mapa.keys().map(String::as_str).collect();
    let lidos = tetos_lidos_pelas_telas();
    assert!(
        !lidos.is_empty(),
        "nenhuma tela lê um teto de `limites_dos_campos`, ou a leitura deste guarda \
             quebrou: ele passaria sem olhar nada"
    );
    let culpados: Vec<_> = lidos
        .iter()
        .filter(|(_, campo)| !chaves.contains(campo.as_str()))
        .collect();
    assert!(
        culpados.is_empty(),
        "estas telas leem um teto de campo que o Rust não serializa, e o que chega é \
             `undefined`, que não confere nada: {culpados:?}. As chaves que existem são \
             {chaves:?}"
    );
    // E todo teto que o Rust manda tem quem o confira: um teto sem leitor é
    // um campo que voltou a mandar sem conferir, ou um teto que sobrou.
    let sem_leitor: Vec<_> = chaves
        .iter()
        .filter(|chave| !lidos.iter().any(|(_, campo)| campo == *chave))
        .collect();
    assert!(
        sem_leitor.is_empty(),
        "nenhuma tela confere estes tetos de `limites_dos_campos`: {sem_leitor:?}. Os \
             lidos são {lidos:?}"
    );
}
