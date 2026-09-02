//! Os scripts de empacotamento não podem ler texto sem dizer o encoding.
//!
//! Isto existe porque não valia. `empacotar/windows.ps1` lia o
//! `tauri.conf.json` com `Get-Content -Raw` e sem `-Encoding`, e o Windows
//! PowerShell 5.1 decodifica assim na página ANSI do sistema — cp1252 numa
//! máquina brasileira. O arquivo é UTF-8 **sem BOM**, e sem BOM não há o que
//! detectar: ele supõe ANSI.
//!
//! O caso que produziu este teste: o título da janela era
//! `SEELE · Entry Plug`, e o `·` é `C2 B7` em UTF-8. Lido como cp1252 vira `Â`
//! seguido de `·`; a escrita, que sempre esteve correta, grava esse par como
//! UTF-8 de verdade, e o arquivo passa a conter `SEELE Â· Entry Plug` — como o
//! script restaura ao sair a mesma string que leu, a corrupção fica **gravada
//! no repositório de quem empacotou**.
//!
//! **O título é só `SEELE` desde o ADR 0035** e não tem mais nenhum byte fora
//! do ASCII, então o caminho exato acima não se reproduz. O defeito não é do
//! `·`: é do `pwsh` supondo ANSI num arquivo sem BOM, e o próximo caractere
//! acentuado que entrar no `tauri.conf.json` o traz de volta. O teste fica, e
//! esta nota existe para que ninguém o apague achando que era sobre o título.
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
    // Os dois arquivos que mandam PowerShell para um Windows: o que roda lá, e o
    // orquestrador daqui, que embute trechos de PowerShell para mandar por SSH.
    // O defeito é do PowerShell 5.1, e não do arquivo — ele viaja com o texto.
    for nome in ["empacotar/windows.ps1", "empacotar/publicar.sh"] {
        let corpo =
            std::fs::read_to_string(raiz().join(nome)).expect("o script tem que ser legível");

        for (numero, linha) in sem_comentario(&corpo).lines().enumerate() {
            if !linha.contains("Get-Content") {
                continue;
            }
            assert!(
                linha.contains("-Encoding") || linha.contains("-Encoding UTF8"),
                "{}:{} lê com Get-Content e não diz o encoding.\n\
                 No PowerShell 5.1 isso decodifica na página ANSI do sistema, e um \
                 arquivo UTF-8 sem BOM volta corrompido:\n  {}",
                nome,
                numero + 1,
                linha.trim()
            );
        }
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
        corpo.contains("SEELE"),
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

// =========================================================================
// `empacotar/publicar.sh` — o orquestrador dos três sistemas.
//
// Ele leva de uma a duas horas para rodar de verdade, e um orquestrador que só
// se prova rodando por noventa minutos não se prova nunca. O que estes testes
// medem é a **decisão**, que é onde os defeitos moram: o que ele confere antes
// de compilar, em que ordem, e o que faz quando um sistema falha e os outros
// dois deram certo. Nada aqui compila nada.
//
// A bancada monta um repositório de mentira — com os empacotadores, o `docker`,
// o `ssh` e o `curl` trocados por dublês que anotam o que foi chamado — e roda o
// script de verdade dentro dele. É a única forma de perguntar «ele parou antes
// de gastar a hora e meia?» e receber uma resposta que não seja opinião.
// =========================================================================

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// O interpretador que roda o `publicar.sh`, achado e não suposto.
///
/// **O Windows não traz um `sh` no PATH, e os testes daqui morriam nele** —
/// «program not found», dez de uma vez, na primeira vez que a bateria daquela
/// máquina chegou a rodar. A saída fácil seria desligá-los ali; seria a errada.
/// É no Windows que os defeitos de fim de linha aparecem, e foi lá que dois
/// guardas deste arquivo já reprovaram por comparar texto com `\n` num
/// repositório que faz checkout com CRLF. Um teste desligado justamente onde ele
/// pega coisa não é um teste.
///
/// O `sh.exe` do Git para Windows roda o script inteiro — medido naquela máquina,
/// com `--decidir`, antes de este caminho existir. Ele só não está no PATH.
fn interpretador() -> PathBuf {
    if !cfg!(windows) {
        return PathBuf::from("sh");
    }
    for caminho in [
        r"C:\Program Files\Git\bin\sh.exe",
        r"C:\Program Files\Git\usr\bin\sh.exe",
        r"C:\Program Files (x86)\Git\bin\sh.exe",
    ] {
        let caminho = PathBuf::from(caminho);
        if caminho.is_file() {
            return caminho;
        }
    }
    // Sem interpretador o teste não roda, e não rodar tem de doer: devolver
    // «sh» aqui deixa o erro ser o mesmo «program not found» de antes, agora com
    // esta função no caminho para quem for procurar o porquê.
    PathBuf::from("sh")
}

fn publicar() -> PathBuf {
    raiz().join("empacotar/publicar.sh")
}

#[test]
fn o_orquestrador_e_shell_posix_valido() {
    // `sh -n` analisa e não executa. É o mesmo portão dos irmãos, e o único que
    // pega um `fi` faltando antes de a pessoa descobrir com o Docker no ar.
    let saida = Command::new(interpretador())
        .arg("-n")
        .arg(publicar())
        .output()
        .expect("o sh tem que executar");

    assert!(
        saida.status.success(),
        "empacotar/publicar.sh não é shell POSIX válido:\n{}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

#[test]
fn o_orquestrador_nao_pode_ganhar_bom() {
    // O oposto exato do `.ps1` irmão, e pela mesma família de motivo: um BOM
    // antes do `#!` faz o núcleo não reconhecer a linha de execução, e a
    // mensagem que sai não sugere nunca que a causa sejam três bytes invisíveis.
    let bruto = std::fs::read(publicar()).expect("legível");
    assert_ne!(
        bruto.get(..3),
        Some(&[0xEF, 0xBB, 0xBF][..]),
        "empacotar/publicar.sh ganhou um BOM, e com ele o shebang deixa de valer"
    );
}

#[test]
fn nenhum_segredo_atravessa_por_argumento() {
    // A chave privada vai para o Windows pela **entrada padrão** do SSH, e é
    // essa a razão de ela poder ficar só no Mac. Se algum dia ela escorregar
    // para a linha de comando, o canal continua cifrado e a propriedade morre do
    // outro lado: no Windows a linha de comando de um processo é legível por
    // outros processos, e o `sshd` com `LogLevel VERBOSE` a escreve no log de
    // eventos.
    //
    // O mesmo vale para o token do GitHub: `-H "Authorization: …"` põe o segredo
    // num `argv` que qualquer `ps` desta máquina lê. Ele entra por `--config -`.
    let corpo = std::fs::read_to_string(publicar()).expect("legível");

    for (numero, linha) in sem_comentario(&corpo).lines().enumerate() {
        let invoca_ssh = linha.contains("ssh ") || linha.contains("scp ");
        assert!(
            !(invoca_ssh && linha.contains("TAURI_SIGNING_PRIVATE_KEY")),
            "empacotar/publicar.sh:{} manda a chave do projeto na linha de comando do \
             ssh.\nEla tem que ir pela entrada padrão:\n  {}",
            numero + 1,
            linha.trim()
        );
        assert!(
            !(linha.contains("-H") && linha.contains("Authorization")),
            "empacotar/publicar.sh:{} põe o token num argumento do curl, e argumento é \
             público para qualquer `ps`.\nEle entra por `--config -`:\n  {}",
            numero + 1,
            linha.trim()
        );
    }
}

// --------------------------------------------------------------- a decisão

/// A regra de publicação, sozinha: sem repositório, sem rede e sem compilar.
fn decidir(pedidos: &str, falhas: &str, parcial: bool) -> (i32, String) {
    let mut comando = Command::new(interpretador());
    comando
        .arg(publicar())
        .arg("--decidir")
        .arg(pedidos)
        .arg(falhas);
    if parcial {
        comando.arg("--parcial");
    }
    let saida = comando.output().expect("o orquestrador tem que executar");
    (
        saida.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&saida.stdout),
            String::from_utf8_lossy(&saida.stderr)
        ),
    )
}

#[test]
fn os_tres_prontos_viram_rascunho() {
    let (estado, texto) = decidir("macos windows linux", "", false);
    assert_eq!(
        estado, 0,
        "com os três prontos não há por que não publicar:\n{texto}"
    );
    assert!(
        texto.starts_with("publicar\n"),
        "a decisão tem que vir na primeira linha:\n{texto}"
    );
}

#[test]
fn um_sistema_faltando_nao_vira_rascunho_calado() {
    // O caso que este script existe para acertar: dois sistemas custaram uma
    // hora e meia e deram certo, o terceiro caiu. Publicar assim mesmo, sem
    // dizer, é o que faz quem usa o sistema que faltou parar de receber
    // atualização sem nunca saber por quê.
    let (estado, texto) = decidir("macos windows linux", "windows", false);
    assert_eq!(
        estado, 1,
        "faltando um sistema, o rascunho não sai sozinho:\n{texto}"
    );
    assert!(
        texto.starts_with("abortar\n"),
        "a decisão tem que vir na primeira linha:\n{texto}"
    );
    assert!(
        texto.contains("continua em entrega/"),
        "quem falhou precisa saber que o que deu certo não foi jogado fora:\n{texto}"
    );
    // E a frase de retomada tem de pular **os que deram certo**: refazer o Linux
    // emulado por causa do Windows é a hora e meia que este script existe para
    // não perder.
    assert!(
        texto.contains("--pular macos,linux"),
        "a retomada tem que dizer quais sistemas pular, e são os que já saíram:\n{texto}"
    );
}

#[test]
fn a_publicacao_parcial_diz_quem_fica_para_tras() {
    let (estado, texto) = decidir("macos windows linux", "windows", true);
    assert_eq!(
        estado, 0,
        "--parcial existe justamente para publicar assim:\n{texto}"
    );
    assert!(
        texto.starts_with("publicar-parcial\n"),
        "a decisão tem que vir na primeira linha:\n{texto}"
    );
    assert!(
        texto.contains("latest.json"),
        "o custo de --parcial é o manifesto sair sem o sistema que faltou, e isso \
         precisa estar dito:\n{texto}"
    );
}

#[test]
fn nada_pronto_nao_publica_nem_com_parcial() {
    // `--parcial` é «publique faltando um», e não «publique nada». Um release
    // vazio é pior que release nenhum: ele existe, tem número, e não entrega.
    let (estado, texto) = decidir("macos windows linux", "macos windows linux", true);
    assert_eq!(estado, 1, "sem nenhum pacote não há release:\n{texto}");
    assert!(
        texto.starts_with("abortar\n"),
        "a decisão tem que vir na primeira linha:\n{texto}"
    );
}

#[test]
fn sem_sistema_pedido_a_publicacao_sozinha_e_legitima() {
    // Retomar só a publicação, depois de a rede cair no meio do envio, não pode
    // exigir refazer duas horas de compilação.
    let (estado, texto) = decidir("", "", false);
    assert_eq!(
        estado, 0,
        "publicar o que já está em entrega/ é caso de uso:\n{texto}"
    );
    assert!(texto.starts_with("publicar\n"), "{texto}");
}

// ----------------------------------------------------- as conferências prévias

/// Um repositório de mentira com dublês no lugar das ferramentas caras.
///
/// O `docker`, o `ssh` e o `curl` viram scripts que anotam a chamada num diário
/// e respondem o que o teste mandar; os empacotadores viram scripts que anotam
/// «empacotei» e nada mais. Assim dá para perguntar a coisa que importa — «ele
/// parou **antes** de começar a compilar?» — sem compilar.
struct Bancada {
    base: PathBuf,
    repo: PathBuf,
    diario: PathBuf,
    corpos: PathBuf,
    commit: String,
}

struct Saida {
    estado: i32,
    texto: String,
    diario: String,
    /// O que o script mandou ao GitHub — o corpo do release, entre outras coisas.
    corpos: String,
}

impl Saida {
    /// Nenhum empacotador foi chamado.
    ///
    /// É a asserção central destes testes. O pior resultado possível do script é
    /// noventa minutos de Linux emulado terminando em «não alcancei o Windows»,
    /// e ela é a única forma de provar que isso não acontece.
    fn nada_foi_empacotado(&self) -> bool {
        !self.diario.contains("empacotei")
    }
}

static CONTADOR: AtomicUsize = AtomicUsize::new(0);

fn escrever(caminho: &PathBuf, conteudo: &str, executavel: bool) {
    if let Some(pai) = caminho.parent() {
        std::fs::create_dir_all(pai).expect("a pasta tem que ser criável");
    }
    std::fs::write(caminho, conteudo).expect("o arquivo tem que ser gravável");
    marcar_executavel(caminho, executavel);
}

/// Marca o arquivo como executável, onde isso quer dizer alguma coisa.
///
/// **Só no Unix, e a razão é do sistema.** No Windows não há bit de execução:
/// quem decide se um arquivo roda é a extensão, e `Permissions` nem tem
/// `from_mode`. Sem esta separação a compilação do `xtask` morria lá com
/// `cannot find `unix` in `os`` — um erro que nenhuma máquina Unix vê, e que só
/// apareceu quando o workspace foi compilado num Windows de verdade.
#[cfg(unix)]
fn marcar_executavel(caminho: &PathBuf, executavel: bool) {
    if executavel {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(caminho, std::fs::Permissions::from_mode(0o755))
            .expect("a permissão tem que colar");
    }
}

/// No Windows não há o que marcar, e fingir que há seria pior que não fazer.
#[cfg(not(unix))]
fn marcar_executavel(_caminho: &PathBuf, _executavel: bool) {}

/// O que falta a esta máquina para rodar o `publicar.sh`, se faltar algo.
///
/// # Por que isto existe
///
/// O script é a ferramenta de quem publica, e quem publica é um Mac. Estes
/// testes o executam de verdade, e para isso precisam do que ele precisa. Na
/// máquina Windows onde a bateria também roda não há Python: o `python3` que
/// aparece no PATH é o **atalho da Microsoft Store** — um executável que existe,
/// responde ao `command -v`, e ao ser chamado imprime uma propaganda da loja e
/// sai. Dez testes reprovavam por isso, cada um dizendo que a conferência que
/// eles guardam tinha falhado.
///
/// **Existir e funcionar não são a mesma coisa**, e é a terceira vez nesta
/// sessão que a diferença custa uma corrida inteira. Por isso a verificação é
/// executar, e não procurar.
///
/// O que se perde ao pular aqui é a execução do script naquela máquina; o que
/// **não** se perde são os guardas de texto deste arquivo, que leem o script
/// como texto e continuam rodando lá — e são eles que pegam fim de linha, que é
/// o defeito que só o Windows mostra.
fn falta_ao_banco() -> Option<String> {
    let interpretador = interpretador();
    if Command::new(&interpretador)
        .arg("-c")
        .arg(":")
        .output()
        .is_err()
    {
        return Some(format!(
            "não há um «sh» executável aqui ({}), e estes testes rodam o publicar.sh",
            interpretador.display()
        ));
    }
    match Command::new("python3").arg("-c").arg("print(1)").output() {
        Err(erro) => Some(format!("o python3 não executa aqui: {erro}")),
        Ok(saida) if !saida.status.success() => Some(format!(
            "o python3 do PATH não é um Python: saiu {} dizendo «{}»",
            saida.status,
            String::from_utf8_lossy(&saida.stdout)
                .lines()
                .chain(String::from_utf8_lossy(&saida.stderr).lines())
                .find(|linha| !linha.trim().is_empty())
                .unwrap_or("nada")
                .trim()
        )),
        Ok(_) => None,
    }
}

impl Bancada {
    /// `None` quando esta máquina não tem com que rodar o script — ver
    /// [`falta_ao_banco`]. Quem chama volta calado do teste **depois** de a
    /// razão ter sido dita na saída de erro.
    fn nova() -> Option<Bancada> {
        if let Some(falta) = falta_ao_banco() {
            eprintln!("PARCIAL: {falta}");
            return None;
        }
        Some(Self::montar())
    }

    fn montar() -> Bancada {
        let base = std::env::temp_dir().join(format!(
            "seele-publicar-{}-{}",
            std::process::id(),
            CONTADOR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("repo");
        let diario = base.join("diario.txt");
        let corpos = base.join("corpos.txt");
        std::fs::create_dir_all(&repo).expect("a bancada tem que ser criável");
        escrever(&diario, "", false);
        escrever(&corpos, "", false);

        // O script de verdade, no lugar de sempre — `RAIZ` sai do caminho dele.
        let corpo = std::fs::read_to_string(publicar()).expect("legível");
        escrever(&repo.join("empacotar/publicar.sh"), &corpo, true);

        // Os empacotadores, que aqui anotam a chamada e deixam em `entrega/` um
        // arquivo com o nome que os de verdade deixariam.
        escrever(
            &repo.join("empacotar/macos.sh"),
            r#"#!/bin/sh
printf 'empacotei macos %s\n' "$1" >> "$SEELE_TESTE_DIARIO"
raiz=$(dirname "$0")/..
# O empacotador de verdade grava a versão aqui e a devolve ao sair. Este morre
# antes de devolver, que é o que acontece quando alguém aperta Ctrl-C.
[ -z "${FALSO_MACOS_SUJA:-}" ] || echo '{"versao gravada": true}' > "$raiz/apps/seele-app/tauri.conf.json"
[ "${FALSO_MACOS:-0}" = 0 ] || exit "${FALSO_MACOS}"
mkdir -p "$raiz/entrega"
echo dmg > "$raiz/entrega/SEELE_$1_aarch64.dmg"
"#,
            true,
        );
        escrever(
            &repo.join("empacotar/linux.sh"),
            r#"#!/bin/sh
printf 'empacotei linux %s\n' "$1" >> "$SEELE_TESTE_DIARIO"
[ "${FALSO_LINUX:-0}" = 0 ] || exit "${FALSO_LINUX}"
raiz=$(dirname "$0")/..
mkdir -p "$raiz/entrega"
echo deb > "$raiz/entrega/seele_$1_amd64.deb"
"#,
            true,
        );
        // O dublê do manifesto **escreve** um latest.json com a casa que lhe
        // pediram. O de verdade põe ali as URLs de download, e é isso que muda de
        // uma casa para a outra; um dublê que só imprimisse deixaria essa troca
        // sem ninguém olhando — e ela é a única coisa que distingue as duas
        // publicações de uma execução.
        escrever(
            &repo.join("empacotar/manifesto.py"),
            r#"#!/usr/bin/env python3
import os, pathlib, sys

entrega, tag = sys.argv[1], sys.argv[2]
repo = sys.argv[sys.argv.index("--repo") + 1]
with open(os.environ["SEELE_TESTE_DIARIO"], "a", encoding="utf-8") as diario:
    diario.write("manifesto " + repo + "\n")
pathlib.Path(entrega, "latest.json").write_text(
    '{"url": "https://github.com/' + repo + '/releases/download/' + tag + '/SEELE.dmg"}',
    encoding="utf-8",
)
"#,
            true,
        );

        // O `tauri.conf.json` como o script espera achá-lo: sem BOM, com o
        // título inteiro e com a metade pública da chave.
        escrever(
            &repo.join("apps/seele-app/tauri.conf.json"),
            "{\n  \"productName\": \"SEELE\",\n  \"app\": { \"windows\": [ { \"title\": \
             \"SEELE\" } ] },\n  \"plugins\": { \"updater\": { \"pubkey\": \
             \"chave-publica-de-mentira\" } }\n}\n",
            false,
        );
        escrever(
            &repo.join(".github/NOTAS-DE-RELEASE.md"),
            "## notas\n",
            false,
        );
        escrever(&repo.join(".gitignore"), "/entrega/\n/target/\n", false);

        // Os dublês das ferramentas caras, fora do repositório para não sujarem
        // a árvore que o próprio script confere.
        escrever(
            &base.join("ferramentas/docker"),
            "#!/bin/sh\nprintf 'docker %s\\n' \"$1\" >> \"$SEELE_TESTE_DIARIO\"\n\
             [ \"${FALSO_DOCKER:-ok}\" = ok ] || exit 1\nexit 0\n",
            true,
        );
        // O dublê do `ssh` decodifica o `-EncodedCommand` para saber qual das
        // três conversas é esta. Sem isso não dá para testar o caminho do
        // Windows inteiro — e é justamente o que não roda na máquina de quem
        // escreve o script.
        escrever(
            &base.join("ferramentas/ssh"),
            r#"#!/bin/sh
printf 'ssh\n' >> "$SEELE_TESTE_DIARIO"
if [ "${FALSO_SSH:-ok}" = recusa ]; then
    echo 'ssh: connect to host port 22: Operation timed out' >&2
    exit 255
fi

codificado=
for argumento in "$@"; do codificado="$argumento"; done
comando=$(printf '%s' "$codificado" | python3 -c 'import base64, sys
sys.stdout.write(base64.b64decode(sys.stdin.read()).decode("utf-16-le"))' 2>/dev/null)

case "$comando" in
    *'fetch --all'*)
        # A troca de commit. Casa por `fetch --all`, e não por `checkout`, porque
        # a sonda também restaura um arquivo com `git checkout --` — casar pelo
        # verbo faria a sonda inteira cair aqui e devolver meia resposta.
        # Devolve a cabeça que a máquina teria DEPOIS de buscar; por padrão a
        # mesma de antes, para o caminho de «não consegui levar» continuar
        # exercitável.
        printf 'head=%s\r\n' "${FALSO_SSH_HEAD_DEPOIS:-${FALSO_SSH_HEAD:-nenhum}}"
        exit 0
        ;;
    *'stash push'*)
        # O guardado. A bancada devolve uma árvore que ficou limpa e uma pilha
        # com um stash — que é o caso bom. `sobrou` diferente de zero é o
        # caminho de «não consegui guardar», e o teste dele encena por aqui.
        printf 'sobrou=%s\r\n' "${FALSO_SSH_SOBROU:-0}"
        printf 'pilha=%s\r\n' "${FALSO_SSH_PILHA:-1}"
        exit 0
        ;;
    *-Versao*)
        cat > /dev/null
        printf 'empacotei windows\n' >> "$SEELE_TESTE_DIARIO"
        exit "${FALSO_WINDOWS:-0}"
        ;;
    *Compress-Archive*)
        python3 -c 'import base64, io, sys, zipfile
memoria = io.BytesIO()
with zipfile.ZipFile(memoria, "w") as pacote:
    pacote.writestr("SEELE_%s_x64-setup.exe" % sys.argv[1], "instalador de mentira")
sys.stdout.write(base64.b64encode(memoria.getvalue()).decode())' "${FALSO_VERSAO:-1.2.3}"
        exit 0
        ;;
esac

printf 'repositorio=%s\r\n' "${FALSO_SSH_REPO:-presente}"
printf 'script=presente\r\n'
printf 'git=presente\r\n'
printf 'cargo=%s\r\n' "${FALSO_SSH_CARGO:-presente}"
printf 'head=%s\r\n' "${FALSO_SSH_HEAD:-nenhum}"
printf 'sujo=%s\r\n' "${FALSO_SSH_SUJO:-nao}"
printf 'restos=%s\r\n' "${FALSO_SSH_RESTOS:-0}"
"#,
            true,
        );
        // O dublê do `curl` guarda o corpo de cada envio: é assim que se lê o
        // que o script diria a quem baixar, sem publicar nada.
        escrever(
            &base.join("ferramentas/curl"),
            r#"#!/bin/sh
cat > /dev/null 2>&1
url=
for argumento in "$@"; do
    case "$argumento" in
        https://*) url="$argumento" ;;
        @*) cat "${argumento#@}" >> "$SEELE_TESTE_CORPOS" ;;
    esac
done
printf 'curl %s\n' "$url" >> "$SEELE_TESTE_DIARIO"

if [ "${FALSO_TOKEN:-bom}" != bom ]; then
    printf '{"message":"Bad credentials"}\n401'
    exit 0
fi

case "$url" in
    */user) printf '{"login":"quem-testa"}\n200' ;;
    */assets*) printf '{"state":"uploaded"}\n201' ;;
    */releases) printf '{"id":42,"html_url":"https://exemplo/releases/untagged"}\n201' ;;
    */releases\?*) printf '%s\n200' "${FALSO_RELEASES:-[]}" ;;
    */commits/*) printf '{}\n%s' "${FALSO_COMMIT:-200}" ;;
    *) printf '{"permissions":{"push":%s}}\n200' "${FALSO_PUSH:-true}" ;;
esac
"#,
            true,
        );

        // Um commit, para que a árvore esteja limpa e haja HEAD a apontar.
        let git = |argumentos: &[&str]| {
            let saida = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(argumentos)
                .output()
                .expect("o git tem que executar");
            assert!(
                saida.status.success(),
                "git {argumentos:?} falhou:\n{}",
                String::from_utf8_lossy(&saida.stderr)
            );
            String::from_utf8_lossy(&saida.stdout).trim().to_owned()
        };
        git(&["init", "-q"]);
        git(&["add", "-A"]);
        git(&[
            "-c",
            "user.email=bancada@seele",
            "-c",
            "user.name=bancada",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "a bancada",
        ]);
        // Um remoto de verdade, num diretório nu ao lado: sem ele o script não
        // tem como conferir se o commit já saiu daqui, e o caminho do empurrão
        // automático ficaria sem bancada.
        let origem = base.join("origem.git");
        let nu = Command::new("git")
            .args(["init", "--bare", "-q"])
            .arg(&origem)
            .output()
            .expect("o git tem que executar");
        assert!(
            nu.status.success(),
            "o remoto de mentira tem que ser criável"
        );
        git(&["remote", "add", "origin", &origem.display().to_string()]);
        git(&["push", "-q", "origin", "HEAD"]);

        let commit = git(&["rev-parse", "HEAD"]);

        Bancada {
            base,
            repo,
            diario,
            corpos,
            commit,
        }
    }

    fn rodar(&self, argumentos: &[&str], ambiente: &[(&str, &str)]) -> Saida {
        let caminho_antigo = std::env::var("PATH").unwrap_or_default();
        let mut comando = Command::new(interpretador());
        comando
            .arg(self.repo.join("empacotar/publicar.sh"))
            .args(argumentos)
            .current_dir(&self.base)
            .env(
                "PATH",
                format!(
                    "{}:{caminho_antigo}",
                    self.base.join("ferramentas").display()
                ),
            )
            .env("SEELE_TESTE_DIARIO", &self.diario)
            .env("SEELE_TESTE_CORPOS", &self.corpos)
            .env("SEELE_GITHUB_TOKEN", "token-de-mentira")
            .env("SEELE_WINDOWS_SSH", "empacotador@windows")
            .env("SEELE_WINDOWS_REPO", "C:\\SEELE")
            .env("TAURI_SIGNING_PRIVATE_KEY", "uma-chave-de-mentira")
            .env("FALSO_SSH_HEAD", &self.commit)
            .env_remove("GITHUB_TOKEN")
            .env_remove("GH_TOKEN")
            .env_remove("TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
        for (chave, valor) in ambiente {
            if valor.is_empty() {
                comando.env_remove(chave);
            } else {
                comando.env(chave, valor);
            }
        }
        let saida = comando.output().expect("o orquestrador tem que executar");
        Saida {
            estado: saida.status.code().unwrap_or(-1),
            texto: format!(
                "{}{}",
                String::from_utf8_lossy(&saida.stdout),
                String::from_utf8_lossy(&saida.stderr)
            ),
            diario: std::fs::read_to_string(&self.diario).unwrap_or_default(),
            corpos: std::fs::read_to_string(&self.corpos).unwrap_or_default(),
        }
    }
}

impl Drop for Bancada {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

#[test]
fn com_tudo_no_lugar_as_conferencias_passam_sem_compilar_nada() {
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--conferir"], &[]);

    assert_eq!(
        saida.estado, 0,
        "com tudo no lugar as conferências têm que passar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("tudo conferido"),
        "faltou o veredito das conferências:\n{}",
        saida.texto
    );
    assert!(
        saida.nada_foi_empacotado(),
        "--conferir compilou alguma coisa:\n{}",
        saida.diario
    );
    // As três respostas caras foram consultadas, e é isso que dá direito a
    // dizer «pode ir». Uma conferência que não pergunta não confere.
    assert!(saida.diario.contains("docker"), "não perguntou ao Docker");
    assert!(saida.diario.contains("ssh"), "não perguntou ao Windows");
    assert!(saida.diario.contains("curl"), "não perguntou ao GitHub");
}

#[test]
fn a_versao_invalida_morre_antes_de_qualquer_pergunta_cara() {
    // A conferência mais barata é a primeira. `0.1.2-rc1` compila duas horas e
    // é recusado pelo empacotador no último passo — este script o recusa no
    // primeiro.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["0.1.2-rc1"], &[]);

    assert_eq!(
        saida.estado, 1,
        "versão inválida tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("não serve para o instalador"),
        "a mensagem tem que dizer o que há de errado com a versão:\n{}",
        saida.texto
    );
    assert!(
        saida.diario.is_empty(),
        "com a versão errada não se pergunta nada a ninguém:\n{}",
        saida.diario
    );
}

#[test]
fn a_arvore_suja_impede_o_empacotamento() {
    // Os empacotadores gravam a versão no `tauri.conf.json` e a devolvem ao
    // sair. Começar sujo é perder a única forma de distinguir resto do script de
    // trabalho de quem estava editando — e foi assim que a versão de um release
    // vazou para um commit.
    // Duas metades, e a segunda é a que a primeira versão deste guarda não tinha.
    //
    // Sujeira **fora** desses caminhos não confunde ninguém, e barrá-la parou o
    // dono na primeira execução por causa de um documento que vive editado — com
    // o script sugerindo `git stash` num arquivo dele. O escopo certo é o que a
    // própria mensagem sempre disse: o que estes scripts reescrevem.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    escrever(
        &bancada.repo.join("um-rascunho.txt"),
        "meu trabalho\n",
        false,
    );
    // `--conferir` porque a pergunta aqui é só «isto barra?». Deixar a execução
    // seguir empacotaria na bancada e sujaria a segunda metade do teste.
    let livre = bancada.rodar(&["1.2.3", "--conferir"], &[]);
    assert!(
        !livre.texto.contains("não commitado"),
        "sujeira fora do que o empacotamento escreve barrou o empacotamento:\n{}",
        livre.texto
    );

    // E agora onde importa.
    escrever(
        &bancada.repo.join("empacotar/rascunho.sh"),
        "# meio de um conserto\n",
        false,
    );
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[]);

    assert_eq!(
        saida.estado, 1,
        "sujeira no que o empacotamento escreve tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("não commitado"),
        "a mensagem tem que dizer o que está sujo:\n{}",
        saida.texto
    );
    assert!(saida.nada_foi_empacotado(), "compilou com a árvore suja");
    // E o trabalho de quem estava editando continua lá: o script não desfaz
    // nada antes de ter mexido em alguma coisa.
    assert!(
        bancada.repo.join("um-rascunho.txt").exists(),
        "o script apagou trabalho que não era dele"
    );
}

#[test]
fn o_docker_fora_do_ar_reprova_antes_da_primeira_compilacao() {
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[("FALSO_DOCKER", "caido")]);

    assert_eq!(
        saida.estado, 1,
        "Docker caído tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("não responde"),
        "a mensagem tem que dizer que o daemon não está no ar:\n{}",
        saida.texto
    );
    assert!(saida.nada_foi_empacotado(), "compilou com o Docker caído");
}

#[test]
fn o_windows_inalcancavel_reprova_antes_do_linux_emulado() {
    // O teste que dá razão a este script existir. Sem ele, a descoberta de que
    // o Windows não atende chega depois de noventa minutos de Linux emulado.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[("FALSO_SSH", "recusa")]);

    assert_eq!(
        saida.estado, 1,
        "Windows fora do ar tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("OpenSSH Server"),
        "a mensagem tem que apontar o recurso que costuma estar desligado:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("administrators_authorized_keys"),
        "a pegadinha da chave de administrador precisa estar na mensagem:\n{}",
        saida.texto
    );
    assert!(
        saida.nada_foi_empacotado(),
        "gastou compilação para descobrir que o Windows não atende:\n{}",
        saida.diario
    );
}

#[test]
fn o_windows_noutro_commit_e_levado_ao_commit_certo() {
    // Antes disto o script parava e mandava rodar `fetch` e `checkout` à mão na
    // outra máquina. Era o passo manual mais caro dos quatro, porque acontecia
    // depois de o SSH já estar de pé — quem publicava descobria que precisava
    // ir até lá tendo tudo pronto para não ir.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(
        &["1.2.3", "--sem-bateria"],
        &[
            ("FALSO_SSH_HEAD", "0000000000000000000000000000000000000000"),
            ("FALSO_SSH_HEAD_DEPOIS", &bancada.commit.clone()),
        ],
    );

    assert!(
        saida.texto.contains("levando") || saida.texto.contains("commit"),
        "o script tem que dizer que está levando a outra máquina ao commit:\n{}",
        saida.texto
    );
    assert!(
        !saida.texto.contains("noutro commit"),
        "divergência que o próprio script resolve não é motivo de parada:\n{}",
        saida.texto
    );
}

#[test]
fn o_windows_que_nao_chega_ao_commit_nao_compila_nada() {
    // O outro lado da mesma moeda: se a troca não pegar — remoto fora do ar,
    // `checkout` recusado, disco cheio —, o script tem de parar. Três pacotes de
    // códigos diferentes são três releases com o mesmo número, e isso não
    // deixou de valer só porque agora a reconciliação é automática.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(
        &["1.2.3", "--sem-bateria"],
        &[("FALSO_SSH_HEAD", "0000000000000000000000000000000000000000")],
    );

    assert_eq!(
        saida.estado, 1,
        "commit que não convergiu tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.nada_foi_empacotado(),
        "compilou com os lados divergentes"
    );
}

#[test]
fn a_chave_pela_metade_reprova_e_sem_assinatura_libera() {
    // Sem a chave não há `latest.json`, e um release sem manifesto deixa todo
    // mundo sem atualização até o seguinte — sem que quem o montou saiba. Ou se
    // tem a chave, ou se diz por escrito que não se quer o botão de atualizar.
    let Some(bancada) = Bancada::nova() else {
        return;
    };

    let saida = bancada.rodar(
        &["1.2.3", "--sem-bateria"],
        &[("TAURI_SIGNING_PRIVATE_KEY", "")],
    );
    assert_eq!(
        saida.estado, 1,
        "meia chave tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida
            .texto
            .contains("chave **privada** não está no ambiente"),
        "a mensagem tem que dizer **qual** metade falta:\n{}",
        saida.texto
    );
    assert!(saida.nada_foi_empacotado(), "compilou sem poder assinar");

    let saida = bancada.rodar(
        &["1.2.3", "--conferir", "--sem-assinatura"],
        &[("TAURI_SIGNING_PRIVATE_KEY", "")],
    );
    assert_eq!(
        saida.estado, 0,
        "--sem-assinatura é a saída escrita para quem aceita o custo:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("ninguém atualiza a partir dele"),
        "o custo de seguir sem assinar tem que estar dito na tela:\n{}",
        saida.texto
    );
}

#[test]
fn o_token_recusado_reprova_antes_de_compilar() {
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[("FALSO_TOKEN", "vencido")]);

    assert_eq!(
        saida.estado, 1,
        "token recusado tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("não aceitou o token"),
        "a mensagem tem que dizer que a autenticação falhou:\n{}",
        saida.texto
    );
    assert!(
        saida.nada_foi_empacotado(),
        "compilou duas horas para descobrir no fim que não pode publicar"
    );
}

#[test]
fn um_token_sem_escrita_reprova() {
    // Ler o repositório e criar release são permissões diferentes, e a segunda é
    // a que só se descobre na hora de publicar.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[("FALSO_PUSH", "false")]);

    assert_eq!(
        saida.estado, 1,
        "token sem escrita tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("não pode escrever"),
        "a mensagem tem que separar «enxerga» de «pode escrever»:\n{}",
        saida.texto
    );
}

#[test]
fn um_release_ja_publicado_nao_e_substituido() {
    // A mesma regra do release.yml: rascunho se substitui sem cerimônia,
    // publicado é decisão que uma pessoa tomou.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(
        &["1.2.3", "--sem-bateria"],
        &[(
            "FALSO_RELEASES",
            "[{\"id\":7,\"tag_name\":\"v1.2.3\",\"draft\":false,\"html_url\":\"https://exemplo\"}]",
        )],
    );

    assert_eq!(
        saida.estado, 1,
        "release publicado tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("**publicado**"),
        "a mensagem tem que dizer que o que existe já foi publicado:\n{}",
        saida.texto
    );
    assert!(
        saida.nada_foi_empacotado(),
        "compilou para não poder publicar"
    );
}

#[test]
fn restos_de_outra_versao_sao_apagados_e_nomeados() {
    // `entrega/` acumula, e tudo o que estiver lá vai para o release: o `.dmg`
    // de 0.9.9 dentro do release de 1.2.3 é uma página que oferece duas versões
    // com o mesmo nome.
    //
    // **Esta decisão foi revertida em 2026-08-20, a pedido de quem publica.**
    // Antes o script parava e mandava mover os arquivos à mão, com o argumento
    // de que «quem apaga entrega passada apaga a entrega que ainda não foi
    // publicada». O argumento continua verdadeiro e o preço foi aceito: parar é
    // um passo manual em toda publicação, e o que se perde é reconstruível a
    // partir do commit que o gerou.
    //
    // O que **não** foi aceito é apagar calado. Cada arquivo removido é
    // nomeado na saída, para quem estiver olhando ver o que sumiu.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    escrever(
        &bancada.repo.join("entrega/SEELE_0.9.9_aarch64.dmg"),
        "de outra vez\n",
        false,
    );
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[]);

    assert!(
        !bancada
            .repo
            .join("entrega/SEELE_0.9.9_aarch64.dmg")
            .exists(),
        "a entrega de outra versão tinha que ter sido apagada:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("SEELE_0.9.9_aarch64.dmg"),
        "o arquivo apagado tem que ser nomeado; apagar calado é pior que parar:\n{}",
        saida.texto
    );
}

#[test]
fn a_entrega_da_versao_corrente_sobrevive_a_limpeza() {
    // Retomar um sistema que falhou é o caso normal deste script, e nele os
    // pacotes dos que deram certo têm de continuar ali — uma limpeza que os
    // levasse junto faria toda retomada recompilar as duas horas que já tinham
    // dado certo.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    escrever(
        &bancada.repo.join("entrega/SEELE_1.2.3_aarch64.dmg"),
        "desta vez\n",
        false,
    );
    escrever(
        &bancada.repo.join("entrega/SEELE_0.9.9_aarch64.dmg"),
        "de outra vez\n",
        false,
    );
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[]);

    assert!(
        bancada
            .repo
            .join("entrega/SEELE_1.2.3_aarch64.dmg")
            .exists(),
        "a limpeza levou junto a entrega desta versão:\n{}",
        saida.texto
    );
    assert!(
        !bancada
            .repo
            .join("entrega/SEELE_0.9.9_aarch64.dmg")
            .exists(),
        "a de outra versão tinha que sair:\n{}",
        saida.texto
    );
}

#[test]
fn o_arquivo_que_o_finder_escreve_nao_e_entrega_de_ninguem() {
    // O `.DS_Store` volta sozinho toda vez que alguém abre a pasta. Nomeá-lo
    // como «apagado» a cada publicação treinaria quem lê a ignorar a lista, que
    // é o que faz a lista deixar de servir para o dia em que ela importar.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    escrever(&bancada.repo.join("entrega/.DS_Store"), "finder\n", false);
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[]);

    assert!(
        !saida.texto.contains(".DS_Store"),
        "o .DS_Store não é entrega de ninguém e não entra na lista:\n{}",
        saida.texto
    );
}

#[test]
fn a_limpeza_do_windows_restaura_so_o_arquivo_conhecido() {
    // A sujeira do Windows é quase sempre um arquivo só: o `windows.ps1` grava
    // a versão no `tauri.release.conf.json` e a devolve ao sair, então uma
    // rodada que morreu no meio deixa ele editado. Restaurar **esse** arquivo é
    // desfazer o que nós fizemos.
    //
    // `reset --hard` e `stash` foram recusados de propósito, e por motivos
    // diferentes: o primeiro apaga trabalho de quem estava naquela máquina, sem
    // aviso e sem volta; o segundo não apaga nada, mas deixa uma entrada de
    // stash por rodada interrompida, e um sedimento que ninguém limpa é o que o
    // ADR 0022 recusou nos mapeamentos de porta permanentes.
    let corpo = std::fs::read_to_string(publicar()).expect("legível");
    let limpo = sem_comentario(&corpo);

    // Só as linhas de PowerShell. Sem este recorte o teste passaria vazio: o
    // `git checkout --` do lado do Mac já existia antes desta mudança (é o
    // mesmo conserto, na máquina de cá), e encontrá-lo não prova nada sobre o
    // que se manda para o Windows.
    let bloco_do_windows: String = limpo
        .lines()
        .filter(|linha| {
            linha.contains("Set-Location")
                || linha.contains("Write-Output")
                || linha.contains("Remove-Item")
                || linha.contains("git ")
                    && (linha.contains("\\$") || linha.contains("REPO_WINDOWS"))
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        bloco_do_windows.contains("git checkout -- '$CONFIG_TAURI'"),
        "o Windows tem que restaurar o arquivo conhecido, e não outra coisa:\n\
         {bloco_do_windows}"
    );
    // Estes dois continuam proibidos, e pelo motivo original: eles **apagam**
    // trabalho de quem está naquela máquina, sem aviso e sem volta.
    for proibido in ["reset --hard", "clean -fd"] {
        assert!(
            !limpo.contains(proibido),
            "«{proibido}» apaga trabalho de quem está naquela máquina"
        );
    }

    // O `git stash` estava nesta lista e saiu. A recusa dele era por sedimento
    // — «uma entrada de stash por rodada interrompida, e um sedimento que
    // ninguém limpa» —, e o preço de mantê-la apareceu no uso: o release
    // parava no meio, e quem publica tinha de ir até a outra máquina limpar à
    // mão **depois** de tudo estar pronto para não ir. A árvore de lá chega
    // suja quase sempre, porque o próprio empacotamento regenera arquivos nela.
    //
    // E havia um motivo que a recusa não via: uma árvore suja no commit certo
    // compila **diferente do commit**, e um release que não sai do código que a
    // tag aponta é pior que sedimento.
    //
    // O sedimento passou a ser tratado em vez de evitado, e é isso que estas
    // duas asserções cobram: a contagem da pilha vai para a tela, e o stash não
    // arrasta arquivo não rastreado — que nunca bloqueou nada e nem muda o que
    // compila.
    assert!(
        limpo.contains("git stash list | Measure-Object"),
        "o stash voltou a crescer calado: sem a contagem da pilha na tela, o \
         sedimento que motivou a recusa original volta inteiro"
    );
    let empurra = limpo
        .lines()
        .find(|linha| linha.contains("git stash push"))
        .unwrap_or_default();
    assert!(
        !empurra.contains(" -u") && !empurra.contains("--include-untracked"),
        "o `git stash push` do Windows leva arquivo não rastreado junto:\n{empurra}"
    );
}

#[test]
fn o_commit_vai_para_o_remoto_antes_de_o_windows_buscar() {
    // O Windows tem que estar no **mesmo commit**, não na ponta do ramo: um
    // release cujos três pacotes vêm de códigos diferentes é três releases com
    // o mesmo número. E um `git pull` lá não alcança um commit que ainda não
    // saiu daqui — por isso o empurrão é deste lado, e antes.
    let corpo = std::fs::read_to_string(publicar()).expect("legível");
    let limpo = sem_comentario(&corpo);

    assert!(
        limpo.contains("ls-remote"),
        "é preciso perguntar se o commit já está no remoto antes de empurrar"
    );
    assert!(
        limpo.contains("git -C \"$RAIZ\" push"),
        "sem o empurrão, o checkout do outro lado busca um commit que não existe lá"
    );
    assert!(
        limpo.contains("checkout") && limpo.contains("fetch"),
        "no Windows é fetch mais checkout do commit, e não pull do ramo"
    );
}

#[test]
fn pular_um_sistema_dispensa_a_ferramenta_dele() {
    // Retomar sem o Docker no ar é o caso de quem já tem o `.deb` da rodada
    // anterior e só precisa refazer o Mac.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(
        &["1.2.3", "--conferir", "--pular", "linux,windows"],
        &[("FALSO_DOCKER", "caido"), ("FALSO_SSH", "recusa")],
    );

    assert_eq!(
        saida.estado, 0,
        "o que foi pulado não pode ser conferido:\n{}",
        saida.texto
    );
    assert!(
        !saida.diario.contains("docker"),
        "perguntou ao Docker mesmo com --pular linux:\n{}",
        saida.diario
    );
    assert!(
        !saida.diario.contains("ssh"),
        "foi ao Windows mesmo com --pular windows:\n{}",
        saida.diario
    );
}

#[test]
fn os_tres_correm_do_mais_barato_para_o_mais_caro() {
    // A ordem não é alfabética nem gosto: o Linux emulado custa uma ordem de
    // grandeza a mais que os outros dois, e o que mais quebra é o código, que
    // quebra igual nos três. O build nativo do Mac primeiro é o que transforma
    // noventa minutos perdidos em cinco.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[]);

    assert_eq!(
        saida.estado, 0,
        "a rodada inteira tinha que passar:\n{}",
        saida.texto
    );
    let macos = saida.diario.find("empacotei macos");
    let windows = saida.diario.find("empacotei windows");
    let linux = saida.diario.find("empacotei linux");
    assert!(
        macos < windows && windows < linux,
        "a ordem tem que ser macOS, Windows, Linux — o caro por último:\n{}",
        saida.diario
    );
    assert!(
        saida.texto.contains("ainda não está publicado"),
        "o fim tem que dizer que ninguém enxerga isto ainda:\n{}",
        saida.texto
    );
}

#[test]
fn o_release_sai_rascunho_e_confessa_a_falta_de_procedencia() {
    // As duas coisas que este release **não** é, ditas onde quem baixa vai ler.
    //
    // O rascunho é o que mantém a decisão de lançar com uma pessoa: o endereço
    // gravado dentro de cada app é `releases/latest`, e `latest` só conta
    // release publicado. E as NOTAS-DE-RELEASE ensinam a conferir a procedência
    // com `gh attestation verify` — que aqui **não acha atestado**, porque não
    // houve workflow. Sem esta confissão no corpo, o comando falha e quem o rodou
    // conclui adulteração onde só houve ausência de CI.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[]);

    assert_eq!(
        saida.estado, 0,
        "a rodada inteira tinha que passar:\n{}",
        saida.texto
    );
    assert!(
        saida.corpos.contains("\"draft\": true"),
        "o release tem que nascer rascunho:\n{}",
        saida.corpos
    );
    assert!(
        saida.corpos.contains("gh attestation verify"),
        "o corpo do release tem que falar do comando que vai falhar:\n{}",
        saida.corpos
    );
    assert!(
        saida.corpos.contains("SHA256SUMS"),
        "e do que continua respondendo «o arquivo chegou inteiro»:\n{}",
        saida.corpos
    );
}

#[test]
fn um_sistema_que_falha_nao_leva_os_outros_junto() {
    // Duas horas de compilação não podem ser jogadas fora porque a primeira
    // delas falhou. O que falha vira uma linha no fim; o que dá certo continua.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[("FALSO_MACOS", "1")]);

    assert_eq!(
        saida.estado, 1,
        "faltando um sistema, o rascunho não sai sem alguém pedir:\n{}",
        saida.texto
    );
    assert!(
        saida.diario.contains("empacotei linux"),
        "parou no primeiro tropeço em vez de seguir com os outros dois:\n{}",
        saida.diario
    );
    assert!(
        saida.texto.contains("Falharam: macos"),
        "o fim tem que nomear quem faltou:\n{}",
        saida.texto
    );
    assert!(
        !saida.diario.contains("SEELE-RELEASES/releases\n"),
        "criou o release mesmo faltando sistema:\n{}",
        saida.diario
    );
}

#[test]
fn com_parcial_o_release_sai_dizendo_quem_faltou() {
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(
        &["1.2.3", "--parcial", "--sem-bateria"],
        &[("FALSO_MACOS", "1")],
    );

    assert_eq!(
        saida.estado, 0,
        "--parcial existe para isto:\n{}",
        saida.texto
    );
    assert!(
        saida.diario.contains("SEELE-RELEASES/releases\n"),
        "com --parcial o rascunho tem que sair:\n{}",
        saida.diario
    );
    assert!(
        saida.texto.contains("faltam sistemas neste release: macos"),
        "publicar faltando um sistema não pode ser silencioso:\n{}",
        saida.texto
    );
}

#[test]
fn a_versao_gravada_nao_fica_no_repositorio() {
    // O empacotador grava a versão no `tauri.conf.json` e a devolve ao sair —
    // com o `trap` dele. Quando ele morre sem chegar lá, alguém tem de devolver,
    // ou o número de um release fica gravado no repositório e entra no próximo
    // commit distraído. Já aconteceu: é a razão de existir o teste
    // `o_titulo_da_janela_atravessou_inteiro`.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let antes = std::fs::read_to_string(bancada.repo.join("apps/seele-app/tauri.conf.json"))
        .expect("legível");

    let saida = bancada.rodar(
        &["1.2.3", "--sem-bateria"],
        &[("FALSO_MACOS", "1"), ("FALSO_MACOS_SUJA", "1")],
    );

    let depois = std::fs::read_to_string(bancada.repo.join("apps/seele-app/tauri.conf.json"))
        .expect("legível");
    assert_eq!(
        antes, depois,
        "o tauri.conf.json ficou com o resto do empacotamento:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("devolvido ao que era"),
        "desfazer calado é quase tão ruim quanto não desfazer:\n{}",
        saida.texto
    );
}

#[test]
fn um_sistema_que_nao_existe_e_recusado() {
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--conferir", "--pular", "bsd"], &[]);

    assert_eq!(saida.estado, 1, "«bsd» não é um dos três:\n{}", saida.texto);
    assert!(
        saida.texto.contains("não conheço o sistema"),
        "um --pular com erro de digitação pularia um sistema em silêncio:\n{}",
        saida.texto
    );
}

// ------------------------------------------------- as notas de uma versão

/// O texto das mudanças, sozinho: sem repositório, sem rede, sem git.
///
/// `notas_das_mudancas` lê assuntos de commit da entrada padrão e escreve
/// markdown na saída. Quem chama o `git log` é uma linha à parte, de propósito:
/// uma função de texto puro se prova alimentando texto, e a página de um release
/// é exatamente o lugar onde ninguém percebe um defeito até ele estar publicado.
fn notas(assuntos: &str) -> String {
    use std::io::Write;
    use std::process::Stdio;

    let mut filho = Command::new(interpretador())
        .arg(publicar())
        .arg("--notas")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("o orquestrador tem que executar");
    if let Some(entrada) = filho.stdin.as_mut() {
        entrada
            .write_all(assuntos.as_bytes())
            .expect("a entrada padrão aceita bytes");
    }
    let saida = filho.wait_with_output().expect("o filho termina");
    String::from_utf8_lossy(&saida.stdout).into_owned()
}

#[test]
fn as_mudancas_separam_o_que_a_pessoa_sente_do_que_e_ferramenta() {
    // A página de um release existe para quem baixa. O conserto do `hdiutil` é
    // verdade e não é notícia para essa pessoa; enterrá-lo seria mentir por
    // omissão, e misturá-lo afoga o que ela veio ler. Duas seções resolvem as
    // duas coisas ao mesmo tempo.
    let texto = notas(
        "feat(ui): a tela para de falar com quem construiu\n\
         fix(empacotar): o rascunho do hdiutil não é pacote\n\
         fix(alcance): as recusas dizem o que houve, e param\n",
    );

    let (em_cima, embaixo) = texto
        .split_once("## Por baixo")
        .unwrap_or_else(|| panic!("as duas seções têm que existir:\n{texto}"));

    assert!(
        em_cima.contains("a tela para de falar com quem construiu")
            && em_cima.contains("as recusas dizem o que houve"),
        "o que a pessoa sente lidera:\n{texto}"
    );
    assert!(
        embaixo.contains("o rascunho do hdiutil não é pacote"),
        "o conserto da ferramenta continua na página, embaixo:\n{texto}"
    );
    assert!(
        !em_cima.contains("hdiutil"),
        "ferramenta não sobe para a primeira seção:\n{texto}"
    );
}

#[test]
fn um_escopo_desconhecido_aparece_em_vez_de_sumir() {
    // O padrão é o lado visível, e a escolha não é arbitrária: deixar um
    // conserto de empacotamento à vista custa uma linha feia; enterrar uma
    // mudança que a pessoa sente custa ela não saber que existe. O erro barato
    // é o que fica sendo o padrão.
    let texto = notas("feat(telepatia): o servidor adivinha o que você ia dizer\n");

    let em_cima = texto
        .split("## Por baixo")
        .next()
        .unwrap_or_default()
        .to_owned();
    assert!(
        em_cima.contains("adivinha o que você ia dizer"),
        "escopo que ninguém classificou ainda tem que aparecer, e aparecer em \
         cima:\n{texto}"
    );
}

#[test]
fn um_escopo_desconhecido_pede_para_ser_classificado() {
    // E o padrão avisa em vez de decidir calado: quem publica vê o nome do
    // escopo novo e classifica. Sem isto, a tabela envelhece sem ninguém
    // perceber, que é como uma decisão de curadoria vira acidente.
    let mut filho = Command::new(interpretador())
        .arg(publicar())
        .arg("--notas")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("o orquestrador tem que executar");
    if let Some(entrada) = filho.stdin.as_mut() {
        use std::io::Write;
        let _ = entrada.write_all(b"feat(telepatia): o servidor adivinha\n");
    }
    let saida = filho.wait_with_output().expect("o filho termina");
    let erro = String::from_utf8_lossy(&saida.stderr);

    assert!(
        erro.contains("telepatia"),
        "o escopo novo tem que ser nomeado no aviso, ou ninguém sabe o que \
         classificar:\n{erro}"
    );
}

#[test]
fn docs_test_e_chore_nao_entram() {
    // Eles são verdade sobre o commit e não são mudança do produto. Quem quiser
    // a verdade completa tem o histórico do git, que continua sendo ela.
    let texto = notas(
        "docs(windows): o icacls usa identificadores\n\
         test(empacotar): a decisão se prova sem compilar nada\n\
         chore: a folha da marca sai do repositório\n\
         refactor(core): o laço fica legível\n",
    );

    for fora in [
        "icacls",
        "se prova sem compilar",
        "folha da marca",
        "laço fica legível",
    ] {
        assert!(
            !texto.contains(fora),
            "«{fora}» não é mudança do produto e não entra na página:\n{texto}"
        );
    }
}

#[test]
fn sem_feat_nem_fix_na_faixa_nao_se_inventa_secao() {
    // Uma versão só de documentação e teste existe, e a página dela tem que
    // dizer isso — não uma seção «O que mudou» vazia, que parece defeito de
    // script para quem lê.
    let texto = notas("docs: só papel\nchore: só arrumação\n");

    assert!(
        !texto.contains("## O que mudou"),
        "seção vazia é pior que seção nenhuma:\n{texto}"
    );
    assert!(
        !texto.contains("## Por baixo"),
        "e a de baixo também não:\n{texto}"
    );
    assert!(
        texto.contains("nenhuma mudança de produto"),
        "o silêncio tem que ser dito, ou parece que o script quebrou:\n{texto}"
    );
}

#[test]
fn o_assunto_atravessa_byte_a_byte() {
    // O caminho é shell, e shell come `$`, barra invertida e crase quando quem
    // escreve não toma cuidado. Um assunto que chega torto à página é o tipo de
    // defeito que só aparece depois de publicado — e as aspas angulares deste
    // projeto e o `»` do empacotador já moraram num commit real.
    let torto = "fix(empacotar): o `»` não entra no $NOME, e a \\ fica";
    let texto = notas(&format!("{torto}\n"));

    assert!(
        texto.contains("o `»` não entra no $NOME, e a \\ fica"),
        "o assunto tem que atravessar sem ser mastigado:\n{texto}"
    );
}

#[test]
fn a_ordem_dentro_de_uma_secao_e_a_que_entrou() {
    // O `git log` vem do mais novo para o mais velho, e a página herda isso sem
    // reordenar: quem acompanha o projeto lê de cima e para quando reconhece.
    let texto = notas(
        "fix(alcance): terceiro\n\
         fix(ui): segundo\n\
         feat(core): primeiro\n",
    );

    let terceiro = texto.find("terceiro");
    let segundo = texto.find("segundo");
    let primeiro = texto.find("primeiro");
    assert!(
        terceiro < segundo && segundo < primeiro,
        "a ordem do histórico tem que sobreviver:\n{texto}"
    );
}

#[test]
fn o_escopo_aparece_ao_lado_do_assunto() {
    // «as recusas dizem o que houve» sozinho fica solto. Com o escopo na frente
    // a frase ganha endereço, e os escopos deste projeto são vocabulário do
    // produto — «alcance», «portaria», «encontro» são as palavras da
    // documentação, não jargão de quem compila.
    let texto = notas("fix(alcance): as recusas dizem o que houve\n");

    assert!(
        texto.contains("**alcance**"),
        "o escopo tem que aparecer, e em negrito:\n{texto}"
    );
}

#[test]
fn um_feat_sem_escopo_nao_e_descartado() {
    // `feat: x` sem parênteses é forma legítima de conventional commit, e um
    // script que a ignorasse perderia mudança sem dizer nada.
    let texto = notas("feat: o SEELE passa a caber num disquete\n");

    assert!(
        texto.contains("caber num disquete"),
        "commit sem escopo é commit:\n{texto}"
    );
}

/// A faixa do release, sozinha: recebe a lista de tags na entrada padrão.
fn anterior(tags: &str, versao: &str) -> String {
    use std::io::Write;
    use std::process::Stdio;

    let mut filho = Command::new(interpretador())
        .arg(publicar())
        .arg("--tag-anterior")
        .arg(versao)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("o orquestrador tem que executar");
    if let Some(entrada) = filho.stdin.as_mut() {
        entrada.write_all(tags.as_bytes()).expect("aceita bytes");
    }
    let saida = filho.wait_with_output().expect("o filho termina");
    String::from_utf8_lossy(&saida.stdout).into_owned()
}

#[test]
fn a_faixa_comeca_na_versao_publicada_antes_desta() {
    assert_eq!(anterior("v0.6.0\nv0.6.1\n0.2.0\n", "0.6.2"), "v0.6.1");
}

#[test]
fn a_versao_sendo_publicada_nao_e_a_faixa_dela_mesma() {
    // `publicar.sh` cria **rascunho**, e rascunho não cria tag — então em uso
    // normal `v$VERSAO` não existe ainda. Mas uma segunda tentativa depois de
    // publicar existe, e aí a tag está lá: sem esta linha a faixa seria
    // `v0.6.2..HEAD`, que é vazia, e a página sairia dizendo que nada mudou.
    assert_eq!(anterior("v0.6.1\nv0.6.2\n", "0.6.2"), "v0.6.1");
}

#[test]
fn a_decima_versao_nao_perde_para_a_nona() {
    // Ordenação de texto poria `v0.9.0` acima de `v0.10.0`, e a faixa do
    // release sairia errada justamente quando o projeto passasse de nove — com
    // o sintoma de a página listar mudanças de versões já publicadas.
    assert_eq!(anterior("v0.9.0\nv0.10.0\nv0.8.3\n", "0.11.0"), "v0.10.0");
}

#[test]
fn sem_tag_anterior_a_faixa_e_vazia_e_isso_nao_e_erro() {
    // A primeira publicação. Devolver vazio deixa quem chama dizer isso na
    // página, em vez de listar o histórico inteiro fingindo que é novidade.
    assert_eq!(anterior("", "0.1.0"), "");
    assert_eq!(anterior("0.2.0\n", "0.1.0"), "", "tag sem «v» não conta");
}

#[test]
fn o_windows_instala_para_a_maquina_e_nao_para_o_usuario() {
    // `currentUser` põe o SEELE em `%LOCALAPPDATA%`, e isso custou caro num
    // teste de campo: ninguém achava o app nem o `connection`, porque as duas coisas
    // que uma pessoa procura primeiro são `Program Files` e o `PATH`, e a
    // instalação por usuário não está em nenhum dos dois.
    //
    // E cobra um segundo preço, que só apareceu depois: sem elevação o
    // instalador não pode criar a regra de firewall de entrada, e no Windows
    // sem essa regra ninguém alcança quem hospeda. `perMachine` pede o UAC uma
    // vez, na instalação, e resolve os dois.
    //
    // O troco é real e está aceito: quem só quer entrar num servidor de outra
    // pessoa também paga o aviso do UAC. Entre pedir uma confirmação e o app
    // não ser encontrável, a confirmação é a mais barata das duas.
    let conf = std::fs::read_to_string(raiz().join("apps/seele-app/tauri.conf.json"))
        .expect("o tauri.conf.json é legível");
    let limpo = sem_comentario(&conf);

    assert!(
        limpo.contains("\"installMode\": \"perMachine\""),
        "o instalador do Windows voltou a ser por usuário, e com ele o app sai \
         de `Program Files` — onde quem procura, procura:\n{limpo}"
    );
}

#[test]
fn o_instalador_do_windows_remove_a_instalacao_por_usuario_de_antes() {
    // A troca para `perMachine` deixou uma ponta solta que só apareceu em campo:
    // o instalador procura a instalação anterior no `HKLM` e a antiga mora no
    // `HKCU`, então ele não a vê. Ficavam duas cópias — `Program Files` e
    // `%LOCALAPPDATA%` — e os atalhos antigos continuavam abrindo a segunda.
    //
    // O relato foi «o aplicativo fica voltando versão»: o atualizador atualiza
    // uma cópia e o atalho abre a outra.
    let conf = std::fs::read_to_string(raiz().join("apps/seele-app/tauri.conf.json"))
        .expect("o tauri.conf.json é legível");
    let limpo = sem_comentario(&conf);
    assert!(
        limpo.contains("\"installerHooks\": \"instalador.nsh\""),
        "o instalador do Windows voltou a não ter gancho, e com ele volta a \
         conviver com a instalação por usuário que ele não enxerga:\n{limpo}"
    );

    let gancho = std::fs::read_to_string(raiz().join("apps/seele-app/instalador.nsh"))
        .expect("o gancho do instalador é legível");
    let corpo: String = gancho
        .lines()
        .filter(|linha| !linha.trim_start().starts_with(';'))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        corpo.contains("ReadRegStr $R7 HKCU"),
        "o gancho parou de procurar no `HKCU`, que é o único lugar onde a \
         instalação por usuário está:\n{corpo}"
    );
    assert!(
        corpo.contains("/S _?="),
        "a remoção da instalação antiga deixou de ser silenciosa e no lugar; \
         sem `_?=` o `ExecWait` não espera, e a instalação nova corre junto \
         com a remoção da velha:\n{corpo}"
    );
    // As duas coisas que o `/UPDATE` estragaria, e a segunda é o motivo de tudo:
    // ele preserva os atalhos, que são exatamente o que precisa sair.
    assert!(
        !corpo.contains("/UPDATE"),
        "o desinstalador passou a ser chamado com `/UPDATE`, que faz ele \
         **manter** os atalhos — que são o que fazia a versão voltar:\n{corpo}"
    );
    // E a que não pode acontecer de jeito nenhum.
    assert!(
        !corpo.contains("$APPDATA") && !corpo.contains("RMDir /r"),
        "o gancho passou a apagar dados: o PERSISTENCE, a identidade e os pinos do \
         ADR 0003 moram aí, e uma migração de instalador não é lugar de \
         perdê-los:\n{corpo}"
    );
}

#[test]
fn o_trabalho_solto_no_windows_e_guardado_e_nao_apagado() {
    // O último passo manual que sobrou dos quatro. `git checkout` recusa uma
    // árvore com alteração no que ele reescreve, e a do Windows chega suja
    // quase sempre — o próprio empacotamento regenera arquivos lá. O release
    // parava no meio, com quem publica tendo de ir até a outra máquina limpar
    // à mão, depois de já ter tudo pronto para não ir.
    //
    // Guardado, e nunca descartado: um `reset --hard` resolveria o mesmo e
    // apagaria trabalho de quem estivesse mexendo naquela máquina.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(
        &["1.2.3", "--sem-bateria"],
        &[
            ("FALSO_SSH_HEAD", &bancada.commit.clone()),
            ("FALSO_SSH_SUJO", "sim"),
        ],
    );

    assert!(
        saida.texto.contains("stash"),
        "o script guardou trabalho da outra máquina e não disse; guardar em \
         silêncio é o mesmo que perder:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("git stash list"),
        "disse que guardou e não disse como recuperar — a pessoa está na \
         máquina errada para descobrir sozinha:\n{}",
        saida.texto
    );

    // E o script que faz isso guarda só o rastreado. Sem `--untracked-files=no`
    // ele relataria «guardei» sobre uma árvore onde só há arquivo novo — que
    // `git stash` sem `-u` não leva, e que nunca bloqueou `checkout` nenhum.
    let script = std::fs::read_to_string(raiz().join("empacotar/publicar.sh"))
        .expect("o publicar.sh é legível");
    // A propriedade é a **ausência** de `-u`, e não a presença de uma bandeira.
    //
    // Havia aqui um `--untracked-files=no`, e ele não existe em `git stash
    // push` — é bandeira de `git status`. O git respondia com o texto de uso, o
    // stash não acontecia, e este teste guardava a coisa quebrada por exigir
    // justamente a bandeira inventada. Encontrado rodando o script contra a
    // máquina Windows de verdade.
    //
    // Sem `-u`, `git stash push` já deixa o não rastreado onde está, que é o
    // que se quer: ele não bloqueia `checkout` nem muda o que compila.
    let empurra = script
        .lines()
        .find(|linha| linha.contains("git stash push"))
        .unwrap_or_default();
    for arrasta in [" -u", "--include-untracked", " -a", "--all"] {
        assert!(
            !empurra.contains(arrasta),
            "o `git stash push` passou a levar arquivo não rastreado junto \
             («{arrasta}»): arrastá-lo tira da outra máquina arquivo que \
             ninguém pediu para guardar:\n{empurra}"
        );
    }
    // Olhando código, e não comentário: os comentários deste bloco discutem o
    // `reset --hard` justamente para dizer por que ele não está lá, e uma busca
    // crua casaria com a explicação e chamaria de defeito.
    let codigo: String = script
        .lines()
        .filter(|linha| !linha.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !codigo.contains("reset --hard"),
        "apareceu um `reset --hard` no caminho do Windows: isso apaga trabalho \
         de quem estiver naquela máquina, e o stash existe para não apagar"
    );
}

#[test]
fn quem_publica_e_quem_atualiza_apontam_para_o_mesmo_repositorio() {
    // Eles se separaram do repositório do código quando ele foi fechado: em
    // repositório privado os anexos de release deixam de ser públicos, e o
    // atualizador faz um GET simples, sem credencial — ele pararia de funcionar
    // em silêncio para todo mundo.
    //
    // Separar é seguro: o pacote é assinado com minisign e a chave pública mora
    // dentro do app, então quem hospeda não precisa ser confiável. O que **não**
    // é seguro é os dois discordarem — o script publicando num lugar e o app
    // procurando noutro. Ninguém percebe até uma versão sair e ninguém receber.
    let script = std::fs::read_to_string(raiz().join("empacotar/publicar.sh"))
        .expect("empacotar/publicar.sh é legível");
    let config = std::fs::read_to_string(raiz().join("apps/seele-app/tauri.conf.json"))
        .expect("tauri.conf.json é legível");

    let destinos = script
        .split("REPOS=\"${SEELE_REPO:-")
        .nth(1)
        .and_then(|resto| resto.split('}').next())
        .expect("o publicar.sh deixou de declarar os repositórios das versões");

    // **Todos, e não só o primeiro.** Enquanto a migração durar são dois, e uma
    // casa que recebe a versão sem constar do `endpoints` é trabalho que ninguém
    // recebe: o release fica lá, completo e assinado, e nenhum app olha para ele.
    for destino in destinos.split_whitespace() {
        assert!(
            config.contains(&format!("github.com/{destino}/releases/")),
            "o script publica em «{destino}» e o `endpoints` do tauri.conf.json \
             não aponta para lá.\n\
             A versão sairia inteira nesse repositório e nenhum app a receberia."
        );
    }
}

#[test]
fn a_versao_sai_nas_duas_casas_numa_execucao_so() {
    // **Compilar uma vez, publicar duas.**
    //
    // Durante a migração cada versão tem de estar nas duas casas: quem instalou
    // o SEELE antes da mudança só conhece a antiga, e a atualização que muda o
    // endereço dele chega por ela. Rodar o script duas vezes daria o mesmo
    // resultado e custaria outra hora e meia de Linux — e, pior, os pacotes da
    // segunda volta seriam outros arquivos, com outras somas, para o mesmo
    // número de versão.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[]);

    assert_eq!(
        saida.estado, 0,
        "a publicação tinha que ir até o fim:\n{}",
        saida.texto
    );
    for casa in ["DATA-AND-DEV/SEELE-RELEASES", "DATA-AND-DEV/SEELE"] {
        assert!(
            saida.diario.contains(&format!("repos/{casa}/releases\n")),
            "não criou o release em {casa}; durante a migração as duas casas \
             precisam da mesma versão:\n{}",
            saida.diario
        );
    }
}

#[test]
fn cada_casa_recebe_o_manifesto_que_aponta_para_ela_mesma() {
    // O `latest.json` carrega as URLs de download dentro dele. Servir da casa
    // antiga um manifesto que aponta para a nova manda o app buscar o pacote num
    // lugar onde ele ainda não pode chegar — e o app não diz o que houve: um
    // download que falha e uma versão que não existe são a mesma tela.
    let Some(bancada) = Bancada::nova() else {
        return;
    };
    let saida = bancada.rodar(&["1.2.3", "--sem-bateria"], &[]);

    for casa in ["DATA-AND-DEV/SEELE-RELEASES", "DATA-AND-DEV/SEELE"] {
        assert!(
            saida.corpos.contains(&format!(
                "https://github.com/{casa}/releases/download/v1.2.3/"
            )),
            "nenhum manifesto subiu apontando para {casa}: alguma casa recebeu o \
             latest.json da outra, e quem atualizar por ela baixa de um lugar \
             onde o arquivo não está."
        );
    }
}

#[test]
fn o_endereco_antigo_continua_na_lista_enquanto_a_migracao_dura() {
    // **Um guarda com prazo, e ele diz isso de si mesmo.**
    //
    // O endereço de atualização é gravado dentro do app instalado. Quem tem o
    // SEELE hoje só conhece `DATA-AND-DEV/SEELE`, e a única forma de mudar esse
    // endereço é uma atualização — que vem por ele. Se aquele repositório
    // deixar de responder antes de todo mundo ter atualizado, quem ficou para
    // trás fica preso para sempre: sem caminho de volta que não seja baixar e
    // instalar à mão.
    //
    // Daí a lista com dois, e nesta ordem: o novo primeiro, porque é para lá
    // que as versões vão; o antigo depois, porque o atualizador tenta em ordem
    // até um responder, e é ele que carrega quem ainda não migrou.
    //
    // **Este teste deve ser apagado**, e por decisão e não por esquecimento:
    // quando o `DATA-AND-DEV/SEELE` parar de receber versões e for razoável
    // considerar que quem ia atualizar já atualizou. Enquanto ele existir,
    // remover o endereço antigo é abandonar gente.
    let config = std::fs::read_to_string(raiz().join("apps/seele-app/tauri.conf.json"))
        .expect("tauri.conf.json é legível");
    assert!(
        config.contains("github.com/DATA-AND-DEV/SEELE/releases/"),
        "o endereço antigo saiu da lista do atualizador antes da migração acabar.\n\
         Quem instalou o SEELE antes da mudança só conhece esse endereço, e a \
         atualização que mudaria o endereço dele vem por ele."
    );
}

#[test]
fn o_que_vem_do_windows_e_lido_byte_a_byte() {
    // O PowerShell manda bytes que não formam texto válido na locale desta
    // máquina, e o `sed` do BSD **aborta** ao topar com um deles: escreve «RE
    // error: illegal byte sequence» na saída de erro e não imprime mais nada,
    // enquanto o script segue como se tivesse lido.
    //
    // Onde isso alimenta uma decisão, o estrago é silencioso: o `head=` diz se o
    // Windows está no commit deste release, e um valor vazio por sed abortado é
    // indistinguível de um Windows no commit errado. Sob `LC_ALL=C` não existe
    // sequência ilegal — byte é byte, e é tudo o que se precisa para achar um
    // prefixo ASCII.
    //
    // Este guarda é de texto, e por isso limitado: ele prende o `LC_ALL=C` no
    // lugar, não prova que a leitura funciona. Provar pediria uma máquina Windows
    // de verdade mandando bytes de verdade, e ela não cabe num teste.
    let script = std::fs::read_to_string(raiz().join("empacotar/publicar.sh"))
        .expect("empacotar/publicar.sh é legível");

    for linha in script.lines() {
        if !linha.contains("$cw_") {
            continue;
        }
        for ferramenta in ["| sed", "| tr"] {
            if linha.contains(ferramenta)
                && !linha.contains(&format!("| LC_ALL=C {}", &ferramenta[2..]))
            {
                panic!(
                    "esta linha lê a saída do Windows sem LC_ALL=C:\n  {}\n\
                     Um byte que não é UTF-8 aborta a leitura sem que o script \
                     perceba, e o que ele decide depois vem de um valor vazio.",
                    linha.trim()
                );
            }
        }
    }
}

#[test]
fn a_bateria_formata_o_workspace_inteiro_e_nao_a_raiz_vazia() {
    // A raiz é um workspace virtual: não tem alvo nenhum para formatar. Sem
    // `--all`, `cargo fmt --manifest-path <raiz>` responde «Failed to find
    // targets» e **sai com erro** — que o script lia como «o código não está
    // formatado», mandando quem publica rodar um `cargo fmt` que não tinha nada
    // para fazer.
    //
    // Custou uma execução inteira, com o Windows já levado ao commit, para
    // descobrir que a queixa não tinha relação com o motivo.
    let script = std::fs::read_to_string(raiz().join("empacotar/publicar.sh"))
        .expect("empacotar/publicar.sh é legível");
    let linha = script
        .lines()
        .find(|linha| linha.contains("cargo fmt") && linha.contains("--check"))
        .expect("a bateria deixou de conferir a formatação");
    assert!(
        linha.contains("--all"),
        "o `cargo fmt` da bateria perdeu o `--all`.\n  {}\n\
         Na raiz de um workspace virtual ele não acha alvo nenhum, sai com erro, \
         e o script culpa a formatação por algo que nunca foi conferido.",
        linha.trim()
    );
}
