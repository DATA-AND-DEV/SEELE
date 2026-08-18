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

fn publicar() -> PathBuf {
    raiz().join("empacotar/publicar.sh")
}

#[test]
fn o_orquestrador_e_shell_posix_valido() {
    // `sh -n` analisa e não executa. É o mesmo portão dos irmãos, e o único que
    // pega um `fi` faltando antes de a pessoa descobrir com o Docker no ar.
    let saida = Command::new("sh")
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
    let mut comando = Command::new("sh");
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
    if executavel {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(caminho, std::fs::Permissions::from_mode(0o755))
            .expect("a permissão tem que colar");
    }
}

impl Bancada {
    fn nova() -> Bancada {
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
[ "${FALSO_MACOS:-0}" = 0 ] || exit "${FALSO_MACOS}"
raiz=$(dirname "$0")/..
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
        escrever(
            &repo.join("empacotar/manifesto.py"),
            "#!/usr/bin/env python3\nprint('manifesto de mentira')\n",
            true,
        );

        // O `tauri.conf.json` como o script espera achá-lo: sem BOM, com o
        // título inteiro e com a metade pública da chave.
        escrever(
            &repo.join("apps/seele-app/tauri.conf.json"),
            "{\n  \"productName\": \"SEELE\",\n  \"app\": { \"windows\": [ { \"title\": \
             \"SEELE · Entry Plug\" } ] },\n  \"plugins\": { \"updater\": { \"pubkey\": \
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
        let mut comando = Command::new("sh");
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
    let bancada = Bancada::nova();
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
    let bancada = Bancada::nova();
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
    let bancada = Bancada::nova();
    escrever(
        &bancada.repo.join("um-rascunho.txt"),
        "meu trabalho\n",
        false,
    );
    let saida = bancada.rodar(&["1.2.3"], &[]);

    assert_eq!(
        saida.estado, 1,
        "árvore suja tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("não está limpa"),
        "a mensagem tem que dizer que a árvore está suja:\n{}",
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
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3"], &[("FALSO_DOCKER", "caido")]);

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
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3"], &[("FALSO_SSH", "recusa")]);

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
fn o_windows_noutro_commit_reprova() {
    // Três pacotes de códigos diferentes são três releases com o mesmo número.
    let bancada = Bancada::nova();
    let saida = bancada.rodar(
        &["1.2.3"],
        &[("FALSO_SSH_HEAD", "0000000000000000000000000000000000000000")],
    );

    assert_eq!(
        saida.estado, 1,
        "commit divergente tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("noutro commit"),
        "a mensagem tem que dizer que os dois lados divergiram:\n{}",
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
    let bancada = Bancada::nova();

    let saida = bancada.rodar(&["1.2.3"], &[("TAURI_SIGNING_PRIVATE_KEY", "")]);
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
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3"], &[("FALSO_TOKEN", "vencido")]);

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
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3"], &[("FALSO_PUSH", "false")]);

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
    let bancada = Bancada::nova();
    let saida = bancada.rodar(
        &["1.2.3"],
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
fn restos_de_outra_versao_nao_sobem_junto() {
    // `entrega/` acumula, e tudo o que estiver lá vai para o release. O `.dmg`
    // de 0.9.9 dentro do release de 1.2.3 é uma página que oferece duas versões
    // com o mesmo nome.
    let bancada = Bancada::nova();
    escrever(
        &bancada.repo.join("entrega/SEELE_0.9.9_aarch64.dmg"),
        "de outra vez\n",
        false,
    );
    let saida = bancada.rodar(&["1.2.3"], &[]);

    assert_eq!(
        saida.estado, 1,
        "resto de outra versão tem que reprovar:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("arquivos de outra versão"),
        "a mensagem tem que nomear o problema:\n{}",
        saida.texto
    );
    // E o arquivo de quem veio antes continua lá: quem apaga entrega passada
    // apaga a entrega que ainda não foi publicada.
    assert!(
        bancada
            .repo
            .join("entrega/SEELE_0.9.9_aarch64.dmg")
            .exists(),
        "o script apagou a entrega de outra versão em vez de reclamar dela"
    );
}

#[test]
fn pular_um_sistema_dispensa_a_ferramenta_dele() {
    // Retomar sem o Docker no ar é o caso de quem já tem o `.deb` da rodada
    // anterior e só precisa refazer o Mac.
    let bancada = Bancada::nova();
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
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3"], &[]);

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
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3"], &[]);

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
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3"], &[("FALSO_MACOS", "1")]);

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
        !saida.diario.contains("SEELE/releases\n"),
        "criou o release mesmo faltando sistema:\n{}",
        saida.diario
    );
}

#[test]
fn com_parcial_o_release_sai_dizendo_quem_faltou() {
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3", "--parcial"], &[("FALSO_MACOS", "1")]);

    assert_eq!(
        saida.estado, 0,
        "--parcial existe para isto:\n{}",
        saida.texto
    );
    assert!(
        saida.diario.contains("SEELE/releases\n"),
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
fn um_sistema_que_nao_existe_e_recusado() {
    let bancada = Bancada::nova();
    let saida = bancada.rodar(&["1.2.3", "--conferir", "--pular", "bsd"], &[]);

    assert_eq!(saida.estado, 1, "«bsd» não é um dos três:\n{}", saida.texto);
    assert!(
        saida.texto.contains("não conheço o sistema"),
        "um --pular com erro de digitação pularia um sistema em silêncio:\n{}",
        saida.texto
    );
}
