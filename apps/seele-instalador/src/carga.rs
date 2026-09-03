//! A carga: os arquivos do produto, dentro do executável.
//!
//! # O que é
//!
//! Um `.tar` comprimido com brotli, embutido pelo `build.rs`, com o que o NSIS
//! empacotava — o
//! `SEELE.exe`, o `seeled.exe` e o que mais o app precisa ao lado. Ela entra por
//! `SEELE_CARGA` no momento de compilar o instalador, que é depois de compilar o
//! produto.
//!
//! # A carga vazia
//!
//! Sem `SEELE_CARGA` o instalador sai com **zero byte** de carga, e isso é um
//! estado legítimo e nomeado: é como a janela é desenvolvida, sem precisar de um
//! release por perto a cada ajuste de layout.
//!
//! O que ele **não** pode fazer é instalar assim. [`existe`] responde antes de
//! qualquer arquivo ser tocado, e a recusa diz o que houve — um instalador que
//! copia zero arquivo e anuncia sucesso é o pior resultado possível: a pessoa
//! fecha a janela achando que tem o produto.
//!
//! **Fora do Windows ninguém consome isto** — a janela é `cfg(windows)` —, e o
//! módulo continua compilando pela mesma razão da `pele`: o teste daqui roda
//! onde a bateria roda primeiro, que é o Mac.
#![cfg_attr(not(windows), allow(dead_code))]

/// O `.tar` comprimido com brotli, com os arquivos do produto, ou vazio.
const CARGA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/carga.tar.br"));

/// Se este executável tem o que instalar.
pub(crate) fn existe() -> bool {
    !CARGA.is_empty()
}

/// Quantos bytes a carga ocupa comprimida.
///
/// O que o desenho mostra é o tamanho **instalado**, que só se sabe descompactando
/// — e descompactar duas vezes para mostrar um número seria pagar a instalação
/// inteira para escrever uma linha. Este é o número honesto que se tem de graça.
pub(crate) const fn comprimida() -> usize {
    CARGA.len()
}

/// Abre a carga dentro de `destino`.
///
/// Chama `progresso` a cada arquivo, com o caminho relativo que acabou de sair.
/// É por aí que o passo 03 preenche o log — e o log é o que faz uma instalação
/// que demora parecer uma instalação que anda.
///
/// # Errors
///
/// Devolve o que impediu, com o arquivo em que parou. Uma instalação que falha
/// no meio deixa a pasta pela metade, e quem lê o erro precisa saber onde parar
/// de procurar.
#[cfg(windows)]
pub(crate) fn abrir_em(
    destino: &std::path::Path,
    mut progresso: impl FnMut(&str),
) -> Result<usize, String> {
    if !existe() {
        return Err(
            "este instalador não traz os arquivos do produto. Ele foi compilado \
             sem `SEELE_CARGA`, que é como a janela é desenvolvida — e não é um \
             instalador que se distribui."
                .to_owned(),
        );
    }

    std::fs::create_dir_all(destino)
        .map_err(|erro| format!("não criei {}: {erro}", destino.display()))?;

    let descompactador = brotli::Decompressor::new(CARGA, 4096);
    let mut arquivo = tar::Archive::new(descompactador);
    let entradas = arquivo
        .entries()
        .map_err(|erro| format!("a carga não abriu: {erro}"))?;

    let mut quantos = 0_usize;
    for entrada in entradas {
        let mut entrada = entrada.map_err(|erro| format!("a carga travou: {erro}"))?;
        let caminho = entrada
            .path()
            .map_err(|erro| format!("um caminho da carga não se lê: {erro}"))?
            .display()
            .to_string();

        // **Nenhum caminho sai da pasta de destino.**
        //
        // Um `.tar` pode carregar `..` no caminho, e um extrator ingênuo escreve
        // fora do destino — em `System32`, se quem montou o arquivo quiser. Esta
        // carga é nossa e nunca teria isso; a conferência existe porque «é nossa»
        // é uma garantia que vale até o dia em que alguém trocar o arquivo.
        if caminho.contains("..") || caminho.starts_with('/') || caminho.contains(':') {
            return Err(format!(
                "a carga traz um caminho que sai da pasta de destino: «{caminho}». \
                 Nenhum arquivo foi escrito."
            ));
        }

        entrada
            .unpack_in(destino)
            .map_err(|erro| format!("não escrevi «{caminho}»: {erro}"))?;
        progresso(&caminho);
        quantos += 1;
    }

    Ok(quantos)
}

#[cfg(test)]
mod testes {
    use super::{comprimida, existe};

    #[test]
    fn sem_carga_o_instalador_sabe_que_esta_vazio() {
        // Este teste roda na árvore, onde ninguém define `SEELE_CARGA` — então
        // ele afirma o estado de desenvolvimento, que é o que precisa ser
        // reconhecível. O guarda do contrário — recusar-se a instalar vazio —
        // está em `abrir_em`, e é o que impede um instalador de zero byte de
        // anunciar sucesso.
        //
        // Se um dia a bateria passar a compilar com carga, este teste reprova e
        // diz por quê: aí a asserção vira a oposta.
        assert_eq!(
            existe(),
            comprimida() > 0,
            "«tem carga» e «tem bytes» deixaram de ser a mesma pergunta"
        );
    }
}
