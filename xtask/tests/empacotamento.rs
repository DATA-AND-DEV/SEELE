//! Os scripts de empacotamento não podem ler texto sem dizer o encoding.
//!
//! Isto existe porque não valia. `empacotar/windows.ps1` lia o
//! `tauri.conf.json` com `Get-Content -Raw` e sem `-Encoding`, e o Windows
//! PowerShell 5.1 decodifica assim na página ANSI do sistema — cp1252 numa
//! máquina brasileira. O arquivo é UTF-8 **sem BOM**, e sem BOM não há o que
//! detectar: ele supõe ANSI.
//!
//! O título da janela é `SEELE · Entry Plug`. O `·` é `C2 B7` em UTF-8; lido
//! como cp1252 vira `Â` seguido de `·`; e a escrita, que sempre esteve correta,
//! grava esse par como UTF-8 de verdade. O arquivo passa a conter
//! `SEELE Â· Entry Plug` — e como o script restaura ao sair a mesma string que
//! leu, a corrupção fica **gravada no repositório de quem empacotou**.
//!
//! Nada disso aparece na máquina de quem escreveu o script: em macOS e Linux o
//! `pwsh` lê UTF-8 por padrão, e o defeito só existe onde o pacote é produzido.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::PathBuf;

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sempre tem pai")
        .to_owned()
}

/// O script sem comentário nem linha em branco.
///
/// Necessário, e não zelo: o comentário que explica este defeito **cita** o
/// `Get-Content -Raw` sem `-Encoding` para dizer o que estava errado. Uma busca
/// no arquivo inteiro seria satisfeita pela própria explicação, que é a forma de
/// guarda que não pode falhar — e já aconteceu três vezes neste repositório num
/// dia só.
fn sem_comentario(texto: &str) -> String {
    texto
        .lines()
        .map(|linha| match linha.find('#') {
            Some(em) => &linha[..em],
            None => linha,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn nenhum_script_le_texto_sem_dizer_o_encoding() {
    let script = raiz().join("empacotar/windows.ps1");
    let corpo = std::fs::read_to_string(&script).expect("o script tem que ser legível");

    for (numero, linha) in sem_comentario(&corpo).lines().enumerate() {
        if !linha.contains("Get-Content") {
            continue;
        }
        assert!(
            linha.contains("-Encoding") || linha.contains("-Encoding UTF8"),
            "empacotar/windows.ps1:{} lê com Get-Content e não diz o encoding.\n\
             No PowerShell 5.1 isso decodifica na página ANSI do sistema, e um \
             arquivo UTF-8 sem BOM volta corrompido:\n  {}",
            numero + 1,
            linha.trim()
        );
    }
}

#[test]
fn o_titulo_da_janela_atravessou_inteiro() {
    // A outra ponta do mesmo defeito, e a que pega o estrago já gravado.
    //
    // Se alguém empacotar num Windows com o script quebrado e commitar o que
    // sobrou, é aqui que aparece — o `Â` perdido é a assinatura exata de UTF-8
    // que passou por uma leitura em Latin-1.
    let config = raiz().join("apps/seele-app/tauri.conf.json");
    let corpo = std::fs::read_to_string(&config).expect("o tauri.conf.json tem que ser legível");

    assert!(
        corpo.contains("SEELE · Entry Plug"),
        "o título da janela não está inteiro no tauri.conf.json"
    );
    assert!(
        !corpo.contains('\u{00C2}'),
        "há um `Â` no tauri.conf.json, que é o rastro de UTF-8 lido como Latin-1 \
         em algum passo do empacotamento"
    );
}

#[test]
fn o_script_do_windows_mantem_o_bom_que_ele_proprio_precisa() {
    // O par do teste acima, e a razão de ele não poder ser «tudo sem BOM».
    //
    // As duas exigências são opostas e verdadeiras ao mesmo tempo: o `.ps1`
    // **precisa** de BOM, senão o 5.1 lê os acentos do próprio script como ANSI
    // e imprime «compilaÃ§Ã£o» na tela; o `.json` **não pode** ter, senão o
    // parser do Tauri tropeça no primeiro byte e diz «expected value at line 1
    // column 1», que não parece ter nada a ver com BOM nenhum.
    //
    // Os dois já quebraram de verdade, um de cada vez.
    let ps1 = std::fs::read(raiz().join("empacotar/windows.ps1")).expect("legível");
    assert_eq!(
        ps1.get(..3),
        Some(&[0xEF, 0xBB, 0xBF][..]),
        "empacotar/windows.ps1 perdeu o BOM, e sem ele o PowerShell 5.1 lê os \
         acentos do script como ANSI"
    );

    let json = std::fs::read(raiz().join("apps/seele-app/tauri.conf.json")).expect("legível");
    assert_ne!(
        json.get(..3),
        Some(&[0xEF, 0xBB, 0xBF][..]),
        "o tauri.conf.json ganhou um BOM, e o Tauri recusa o arquivo assim"
    );
}

#[test]
fn a_chave_publica_do_atualizador_decodifica_no_que_ela_diz_ser() {
    // Vazia é estado legítimo — é o que diz «este build não atualiza ninguém», e
    // o app tem frase própria para isso. O que não pode é ser **quase** uma
    // chave.
    //
    // Isto existe porque aconteceu. A chave foi colada com um `%` no fim: o
    // marcador que o zsh imprime quando a saída não termina em nova linha, e que
    // vem junto numa cópia feita do terminal. Com ele o valor não é base64
    // válido, e a chave pública é **compilada dentro de cada executável** — o
    // erro viajaria para o computador de cada pessoa antes de alguém notar.
    //
    // Uma linha a mais de descuido, e o conserto seria pedir a todo mundo que
    // reinstalasse à mão: exatamente o problema que o atualizador existe para
    // acabar.
    let config = raiz().join("apps/seele-app/tauri.conf.json");
    let corpo = std::fs::read_to_string(&config).expect("o tauri.conf.json tem que ser legível");

    let Some(depois) = corpo.split("\"pubkey\": \"").nth(1) else {
        panic!("o tauri.conf.json não tem campo `pubkey`");
    };
    let chave = depois.split('"').next().unwrap_or_default();

    if chave.is_empty() {
        return; // sem chave, e isso é dito na tela
    }

    let bytes = base64_decodificar(chave).unwrap_or_else(|erro| {
        panic!(
            "a `pubkey` não é base64 válido ({erro}).\n\
             O suspeito de sempre é um caractere a mais colado do terminal — o \
             `%` do zsh, um espaço, uma quebra de linha.\n\
             valor: {chave}"
        )
    });
    let texto = String::from_utf8(bytes).expect("uma chave minisign é texto");

    assert!(
        texto.starts_with("untrusted comment:"),
        "a `pubkey` decodifica, mas não no que uma chave minisign é:\n{texto}"
    );
    assert_eq!(
        texto.lines().count(),
        2,
        "uma chave minisign tem duas linhas — o comentário e a chave:\n{texto}"
    );
}

/// Base64 padrão, sem dependência.
///
/// Trinta linhas contra uma crate nova num `xtask` que hoje não tem nenhuma: a
/// conta é a mesma que o resto do repositório faz, e aqui ela dá para este lado.
fn base64_decodificar(entrada: &str) -> Result<Vec<u8>, String> {
    const ALFABETO: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut acumulado: u32 = 0;
    let mut bits = 0_u32;
    let mut saida = Vec::new();

    for (posicao, byte) in entrada.bytes().enumerate() {
        if byte == b'=' {
            break;
        }
        let valor = ALFABETO
            .iter()
            .position(|candidato| *candidato == byte)
            .ok_or_else(|| format!("caractere `{}` na posição {posicao}", byte as char))?;
        acumulado = (acumulado << 6) | u32::try_from(valor).map_err(|erro| erro.to_string())?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            let oitava = u8::try_from((acumulado >> bits) & 0xFF).map_err(|e| e.to_string())?;
            saida.push(oitava);
        }
    }

    Ok(saida)
}
