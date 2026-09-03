//! Embute o ícone no executável.
//!
//! # Por que um `build.rs` para isto
//!
//! No Windows o ícone que o Explorer mostra não é um arquivo ao lado do
//! programa: é um **recurso dentro do `.exe`**, e recurso se compila junto. Sem
//! isso o instalador aparece com o ícone genérico de aplicativo — na tela em que
//! alguém decide se confia no arquivo que acabou de baixar.
//!
//! # Por que `embed-resource`
//!
//! Porque ele **já está na árvore**: o `tauri-build` o traz para embutir o
//! ícone do próprio SEELE. Usá-lo aqui não acrescenta uma linha ao grafo de
//! dependências nem uma superfície nova para auditar — que é o critério do ADR
//! 0019 para o que entra.

fn main() {
    carga();

    // O recurso só existe no Windows. Fora dele o `build.rs` não faz nada, e a
    // crate continua compilando no Mac — que é onde a bateria roda primeiro.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=icone.rc");
        println!("cargo:rerun-if-changed=seele.manifest");
        println!("cargo:rerun-if-changed=../seele-app/icons/icon.ico");
        // O `expect` é negado pela folha de lints, e aqui a alternativa é
        // melhor mesmo: um `panic!` de `build.rs` sai como rastro de pânico do
        // Rust no meio da compilação, e `panic!` com a frase certa sai como a
        // frase certa.
        if let Err(erro) =
            embed_resource::compile("icone.rc", embed_resource::NONE).manifest_optional()
        {
            panic!(
                "o ícone não entrou no executável: {erro}\n\
                 Sem ele o instalador aparece com o ícone genérico de \
                 aplicativo — na tela em que alguém decide se confia no arquivo \
                 que acabou de baixar."
            );
        }
    }
}

/// Comprime a carga — os arquivos do produto — onde o `include_bytes!` a encontra.
///
/// # Por que por variável de ambiente
///
/// A carga é o resultado de compilar o SEELE inteiro, e o instalador é compilado
/// depois dele. Um caminho fixo no código obrigaria a árvore a ter sempre um
/// `.tar.gz` de release por perto — inclusive para quem só quer mexer na janela.
///
/// Sem `SEELE_CARGA`, entra uma carga **vazia** e o instalador se recusa a
/// instalar, dizendo isso. É o estado em que a janela é desenvolvida, e é
/// melhor que um instalador que compila igual nos dois casos e só descobre o
/// vazio na máquina de quem baixou.
fn carga() {
    println!("cargo:rerun-if-env-changed=SEELE_CARGA");
    let destino =
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap_or_default()).join("carga.tar.br");

    match std::env::var("SEELE_CARGA") {
        Ok(origem) if !origem.is_empty() => {
            println!("cargo:rerun-if-changed={origem}");
            comprimir(&origem, &destino);
        }
        _ => {
            // Vazia, e não ausente: o `include_bytes!` precisa de um arquivo, e
            // um arquivo de zero byte é o que faz o instalador saber que não tem
            // o que instalar.
            if let Err(erro) = std::fs::write(&destino, []) {
                panic!("não criei a carga vazia: {erro}");
            }
        }
    }
}

/// Comprime o `.tar` com brotli, na qualidade máxima.
///
/// # Por que aqui e não no empacotamento
///
/// Porque o compressor já está no `Cargo.lock` e não existe como programa na
/// máquina que empacota: o `tar` do Windows faz `.tar` e `.tar.gz`, e não
/// brotli. Pedir um binário a mais só para comprimir seria uma dependência
/// externa para uma coisa que a árvore já sabe fazer.
///
/// Qualidade 11 e janela de 24 bits, que é o máximo. Isto roda uma vez por
/// pacote, e o que se ganha é baixado por todo mundo em toda atualização —
/// trocar minutos de quem empacota por megabytes de quem instala é fácil.
fn comprimir(origem: &str, destino: &std::path::Path) {
    let cru = match std::fs::read(origem) {
        Ok(bytes) => bytes,
        Err(erro) => panic!(
            "não li a carga de «{origem}»: {erro}\n\
             `SEELE_CARGA` aponta para o `.tar` com os arquivos do produto. Se \
             ele não existe, o instalador sairia vazio."
        ),
    };

    let saida = match std::fs::File::create(destino) {
        Ok(arquivo) => arquivo,
        Err(erro) => panic!("não criei {}: {erro}", destino.display()),
    };
    let mut comprimindo = brotli::CompressorWriter::new(saida, 4096, 11, 24);
    if let Err(erro) = std::io::Write::write_all(&mut comprimindo, &cru) {
        panic!("não comprimi a carga: {erro}");
    }
    if let Err(erro) = std::io::Write::flush(&mut comprimindo) {
        panic!("não fechei a carga comprimida: {erro}");
    }
}
