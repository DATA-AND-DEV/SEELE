//! **Se o firewall do Windows deixa chegar conexão a este SEELE** — lido num
//! lugar só (D-sala-1; a conferência no PC de testes, 15/10).
//!
//! # Por que um lugar só, e por que este
//!
//! Eram dois leitores, e nenhum lia a regra. O do app perguntava ao `netsh` se
//! existia **alguma regra chamada `SEELE`**, pelo código de saída: um bloqueio
//! com esse nome — o aviso do próprio Windows dá às regras o nome do produto, e
//! um «Cancelar» nele cria um bloqueio — passava por coberto. O do
//! `seele-server` (`alcance::firewall`) não era chamado por ninguém fora dos
//! testes, comparava só o **nome do arquivo** do programa e tinha um teste
//! afirmando que uma regra para `D:\Programas\SEELE\seele.exe` libera
//! `C:\Program Files\SEELE\SEELE.exe` — e o firewall casa o caminho inteiro.
//!
//! # Por que o PowerShell, e não o `netsh`
//!
//! O `netsh` traduz rótulo **e** valor: no PC de testes, «Habilitado: Sim»,
//! «Direção: Entrada», «Ação: Permitir». Ler isso é adivinhar o idioma da
//! máquina de outra pessoa. O `Get-NetFirewallRule` devolve os enums com os
//! mesmos nomes em qualquer idioma (medido: `pt-BR`, PowerShell 5.1, 158 ms com
//! a máquina parada, 977 ms compilando). A pergunta roda uma vez, quando a sala
//! sobe ou quando se hospeda, em segundo plano.
//!
//! # O que conta, e na ordem do firewall
//!
//! - só regra **ligada**, de **entrada**, **UDP** ou de qualquer protocolo, e
//!   do **caminho inteiro** do programa que roda — sem diferença de caixa (as
//!   regras do aviso do Windows guardam o caminho em minúsculas) e com as
//!   variáveis expandidas;
//! - o **perfil** da regra tem de cobrir alguma rede ativa: as regras do aviso
//!   do Windows no PC de testes eram só do `Public`, e uma regra assim não vale
//!   numa rede `Private`;
//! - **bloqueio vence permissão**, como no firewall.
//!
//! E **não adivinha**: uma leitura que falhou, que não chegou ao fim ou que não
//! se entende é [`Entrada::NaoSei`], que nunca vira frase de acusação — vai ao
//! `seele.log` com o motivo.

// Fora do Windows a leitura do sistema não roda — o firewall que barra entrada
// de um programa que escuta é o dele —, mas a decisão continua compilada, para
// os testes a provarem com a saída de verdade do PC de testes.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::Path;

/// O que se sabe da entrada de fora para este programa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entrada {
    /// Uma regra ligada, de entrada, UDP, deste programa, cobre a rede ativa.
    Liberada,
    /// Uma regra que **bloqueia** este programa cobre a rede ativa.
    Bloqueada {
        /// O nome da regra, como o Windows a mostra.
        regra: String,
    },
    /// Há regra que libera este programa, mas só em perfis de rede que não são
    /// os da rede ativa.
    SoNoPerfil {
        /// Os perfis que a regra cobre (`Public`, `Private`, `Domain`).
        perfis: String,
        /// As categorias das redes ativas.
        redes: String,
    },
    /// Nenhuma regra é deste caminho, mas uma libera outra cópia do mesmo
    /// programa — a instalada, quando esta é o pacote portátil.
    OutraCopia {
        /// O caminho que a regra libera.
        coberta: String,
    },
    /// Nenhuma regra libera este programa.
    SemRegra,
    /// Não deu para saber.
    NaoSei {
        /// Por quê, para o `seele.log`.
        motivo: String,
    },
}

/// Uma regra, como o script a escreve: os enums em texto, que não se traduz.
#[derive(Debug, serde::Deserialize)]
struct Regra {
    #[serde(default)]
    nome: String,
    ligada: String,
    direcao: String,
    acao: String,
    protocolo: String,
    perfil: String,
    programa: String,
}

/// O script que o PowerShell roda, em `-EncodedCommand` para não depender de
/// como o Windows cita argumentos.
///
/// Uma linha por coisa — `regra <json>`, `rede <categoria>` — e `fim` no fim,
/// em vez de um JSON só: o `ConvertTo-Json` do PowerShell 5.1 devolve um objeto,
/// e não uma lista, quando há um item só (medido no PC de testes), e às vezes
/// embrulha a lista em `{"value": …, "Count": …}`. O `fim` é a prova de que o
/// script chegou até o final.
pub(crate) const SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$nome = '*\' + [IO.Path]::GetFileName($env:SEELE_EXE)
Get-NetFirewallApplicationFilter -PolicyStore ActiveStore |
  Where-Object { [Environment]::ExpandEnvironmentVariables($_.Program) -like $nome } |
  ForEach-Object {
    $r = $_ | Get-NetFirewallRule
    $f = $r | Get-NetFirewallPortFilter
    'regra ' + ([pscustomobject]@{
      nome = [string]$r.DisplayName; ligada = [string]$r.Enabled; direcao = [string]$r.Direction
      acao = [string]$r.Action; protocolo = [string]$f.Protocol; perfil = [string]$r.Profile
      programa = [Environment]::ExpandEnvironmentVariables($_.Program)
    } | ConvertTo-Json -Compress)
  }
Get-NetConnectionProfile | ForEach-Object { 'rede ' + [string]$_.NetworkCategory }
'fim'
"#;

/// Lê a saída do [`SCRIPT`] e decide para o executável `exe`.
pub(crate) fn ler(saida: &str, exe: &Path) -> Entrada {
    let mut regras = Vec::new();
    let mut redes = Vec::new();
    let mut terminou = false;
    for linha in saida.lines() {
        let linha = linha.trim_start_matches('\u{feff}').trim();
        if let Some(json) = linha.strip_prefix("regra ") {
            match serde_json::from_str::<Regra>(json) {
                Ok(regra) => regras.push(regra),
                Err(erro) => {
                    return Entrada::NaoSei {
                        motivo: format!("uma regra do firewall não se leu ({erro}): {json}"),
                    }
                }
            }
        } else if let Some(rede) = linha.strip_prefix("rede ") {
            redes.push(rede.trim().to_owned());
        } else if linha == "fim" {
            terminou = true;
        }
    }
    if !terminou {
        return Entrada::NaoSei {
            motivo: "a leitura do firewall não chegou ao fim".to_owned(),
        };
    }
    decidir(&regras, &redes, exe)
}

/// O caminho como o firewall o compara: sem caixa, sem aspas, com a barra do
/// Windows e sem o prefixo `\\?\` que o `current_exe` às vezes traz.
fn normalizar(caminho: &str) -> String {
    let caminho = caminho.trim().trim_matches('"');
    let caminho = caminho.strip_prefix(r"\\?\").unwrap_or(caminho);
    caminho.replace('/', r"\").to_lowercase()
}

/// Se o perfil da regra cobre alguma das redes ativas: `None` quando não se
/// sabe a rede e a regra não vale em todas.
fn cobre(perfil: &str, redes: &[String]) -> Option<bool> {
    if perfil.trim().eq_ignore_ascii_case("Any") {
        return Some(true);
    }
    if redes.is_empty() {
        return None;
    }
    let perfis: Vec<String> = perfil
        .split(',')
        .map(|p| p.trim().to_ascii_lowercase())
        .collect();
    Some(redes.iter().any(|rede| {
        // `DomainAuthenticated` é a categoria da rede; `Domain`, o perfil.
        let rede = match rede.trim() {
            r if r.eq_ignore_ascii_case("DomainAuthenticated") => "domain".to_owned(),
            r => r.to_ascii_lowercase(),
        };
        perfis.contains(&rede)
    }))
}

fn decidir(regras: &[Regra], redes: &[String], exe: &Path) -> Entrada {
    let eu = normalizar(&exe.to_string_lossy());
    let vale = |regra: &&Regra| {
        regra.ligada.eq_ignore_ascii_case("True")
            && regra.direcao.eq_ignore_ascii_case("Inbound")
            && (regra.protocolo.eq_ignore_ascii_case("UDP")
                || regra.protocolo.eq_ignore_ascii_case("Any"))
    };
    let deste = |regra: &&Regra| normalizar(&regra.programa) == eu;
    // «Outra cópia» é o mesmo programa noutro caminho: o mesmo nome de
    // arquivo. O script já filtra por ele, mas a decisão não depende disso.
    let nome_de = |caminho: &str| caminho.rsplit('\\').next().unwrap_or_default().to_owned();
    let meu_nome = nome_de(&eu);
    let outra_copia = |regra: &&Regra| {
        let dela = normalizar(&regra.programa);
        dela != eu && nome_de(&dela) == meu_nome
    };
    let bloqueia = |regra: &&Regra| regra.acao.eq_ignore_ascii_case("Block");
    let permite = |regra: &&Regra| regra.acao.eq_ignore_ascii_case("Allow");

    // **Bloqueio vence permissão.** Um bloqueio cuja rede não se sabe também
    // conta: dizer «liberada» ao lado dele seria a mentira que este módulo
    // existe para não contar.
    if let Some(regra) = regras
        .iter()
        .filter(vale)
        .filter(deste)
        .filter(bloqueia)
        .find(|regra| cobre(&regra.perfil, redes) != Some(false))
    {
        return Entrada::Bloqueada {
            regra: regra.nome.clone(),
        };
    }
    let permitem: Vec<&Regra> = regras
        .iter()
        .filter(vale)
        .filter(deste)
        .filter(permite)
        .collect();
    if permitem
        .iter()
        .any(|regra| cobre(&regra.perfil, redes) == Some(true))
    {
        return Entrada::Liberada;
    }
    if permitem
        .iter()
        .any(|regra| cobre(&regra.perfil, redes).is_none())
    {
        return Entrada::NaoSei {
            motivo: "há regra que libera este SEELE só em alguns perfis, e a rede ativa não se leu"
                .to_owned(),
        };
    }
    if let Some(regra) = permitem.first() {
        return Entrada::SoNoPerfil {
            perfis: regra.perfil.clone(),
            redes: redes.join(", "),
        };
    }
    if let Some(regra) = regras
        .iter()
        .filter(vale)
        .filter(permite)
        .find(|regra| outra_copia(regra))
    {
        return Entrada::OutraCopia {
            coberta: regra.programa.clone(),
        };
    }
    Entrada::SemRegra
}

/// O perfil e a categoria de rede, como a pessoa os lê na tela do Windows.
fn perfil_por_extenso(nomes: &str) -> String {
    nomes
        .split(',')
        .map(|nome| match nome.trim() {
            n if n.eq_ignore_ascii_case("Public") => "pública",
            n if n.eq_ignore_ascii_case("Private") => "particular",
            n if n.eq_ignore_ascii_case("Domain")
                || n.eq_ignore_ascii_case("DomainAuthenticated") =>
            {
                "de domínio"
            }
            outro => outro,
        })
        .collect::<Vec<_>>()
        .join(" ou ")
}

/// A frase que a tela mostra, ou `None` quando não há o que dizer.
///
/// **Diz o que a regra é**: o SEELE liberado no firewall, pelo programa, em
/// UDP — e não uma porta, porque a regra não é de porta e cobre a 8383 do
/// servidor guardado e a 8385 da sala ao mesmo tempo. `NaoSei` não vira frase:
/// acusar sem saber manda a pessoa mexer em segurança à toa.
pub(crate) fn frase(entrada: &Entrada, exe: &Path) -> Option<String> {
    let exe = exe.display();
    match entrada {
        Entrada::Liberada | Entrada::NaoSei { .. } => None,
        Entrada::SemRegra => Some(format!(
            "o firewall do Windows não tem uma regra que libere este SEELE ({exe}) para \
             receber conexões em UDP: quem está fora da sua rede, e até quem está nela, \
             não chega."
        )),
        Entrada::Bloqueada { regra } => Some(format!(
            "o firewall do Windows tem uma regra que bloqueia este SEELE («{regra}»), e um \
             bloqueio vale mais que qualquer liberação: ninguém de fora chega. Ela costuma \
             nascer de um «Cancelar» no aviso do Windows, e sai nas configurações do \
             firewall."
        )),
        Entrada::SoNoPerfil { perfis, redes } => Some(format!(
            "o firewall do Windows libera este SEELE só em rede {}, e a rede de agora é {}: \
             quem está fora não chega por ela.",
            perfil_por_extenso(perfis),
            perfil_por_extenso(redes)
        )),
        Entrada::OutraCopia { coberta } => Some(format!(
            "o firewall do Windows libera o SEELE de {coberta}, e este é outro ({exe}): \
             conexões de fora não chegam a esta cópia."
        )),
    }
}

/// Base64 do texto em UTF-16LE, que é o que o `-EncodedCommand` do PowerShell
/// pede. À mão, e com um teste contra o `base64`: o crate só existe para os
/// testes, e uma dependência nova do app por vinte linhas não se paga.
pub(crate) fn comando_codificado(script: &str) -> String {
    const LETRAS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut saida = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for trio in bytes.chunks(3) {
        let [a, b, c] = [
            trio.first().copied().unwrap_or(0),
            trio.get(1).copied().unwrap_or(0),
            trio.get(2).copied().unwrap_or(0),
        ];
        let numero = (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c);
        for (posicao, deslocamento) in [18_u32, 12, 6, 0].into_iter().enumerate() {
            if posicao > trio.len() {
                saida.push('=');
            } else {
                let indice = usize::try_from((numero >> deslocamento) & 0x3f).unwrap_or(0);
                saida.push(char::from(LETRAS.get(indice).copied().unwrap_or(b'A')));
            }
        }
    }
    saida
}

/// Pergunta ao Windows, pelo PowerShell, e decide para o executável que roda.
#[cfg(windows)]
pub(crate) fn ler_deste_programa() -> Entrada {
    use std::os::windows::process::CommandExt;

    let Ok(eu) = std::env::current_exe() else {
        return Entrada::NaoSei {
            motivo: "o caminho deste executável não se leu".to_owned(),
        };
    };
    // O caminho inteiro, como o instalador faz com o `netsh`: resolver o nome
    // pelo PATH é deixar quem escreve no PATH escolher o que roda.
    let powershell =
        Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned()))
            .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let saida = std::process::Command::new(powershell)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &comando_codificado(SCRIPT),
        ])
        .env("SEELE_EXE", &eu)
        .creation_flags(crate::SEM_CONSOLE)
        .output();
    match saida {
        Ok(saida) if saida.status.success() => ler(&String::from_utf8_lossy(&saida.stdout), &eu),
        Ok(saida) => Entrada::NaoSei {
            motivo: format!(
                "o PowerShell saiu com {}: {}",
                saida.status,
                String::from_utf8_lossy(&saida.stderr).trim()
            ),
        },
        Err(erro) => Entrada::NaoSei {
            motivo: format!("o PowerShell não abriu: {erro}"),
        },
    }
}

/// Fora do Windows não há o que perguntar, e a resposta honesta é essa.
#[cfg(not(windows))]
pub(crate) fn ler_deste_programa() -> Entrada {
    Entrada::NaoSei {
        motivo: "não é o Windows".to_owned(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// **A saída de verdade**, do PC de testes em 15/10 (`pt-BR`, PowerShell
    /// 5.1): a regra do instalador e o par TCP/UDP que o aviso do Windows criou
    /// para um `seele-app.exe` de desenvolvimento, em minúsculas e só no perfil
    /// público. Reescrita no formato de linhas do [`SCRIPT`].
    const DO_PC: &str = r#"regra {"nome":"seele-app.exe","ligada":"True","direcao":"Inbound","acao":"Allow","protocolo":"TCP","perfil":"Public","programa":"C:\\users\\aleta\\appdata\\local\\seele\\seele-app.exe"}
regra {"nome":"seele-app.exe","ligada":"True","direcao":"Inbound","acao":"Allow","protocolo":"UDP","perfil":"Public","programa":"C:\\users\\aleta\\appdata\\local\\seele\\seele-app.exe"}
regra {"nome":"SEELE","ligada":"True","direcao":"Inbound","acao":"Allow","protocolo":"UDP","perfil":"Any","programa":"C:\\Program Files\\SEELE\\SEELE.exe"}
rede Private
fim
"#;

    const INSTALADO: &str = r"C:\Program Files\SEELE\SEELE.exe";

    fn com(linhas: &str) -> String {
        format!("{linhas}\nrede Private\nfim\n")
    }

    #[test]
    fn a_regra_do_instalador_libera_o_programa_instalado() {
        assert_eq!(ler(DO_PC, Path::new(INSTALADO)), Entrada::Liberada);
    }

    #[test]
    fn um_bloqueio_chamado_seele_nao_passa_por_coberto() {
        // **O defeito da leitura de antes**, que só perguntava se existia uma
        // regra com este nome.
        let bloqueio = com(
            r#"regra {"nome":"SEELE","ligada":"True","direcao":"Inbound","acao":"Block","protocolo":"UDP","perfil":"Any","programa":"C:\\Program Files\\SEELE\\SEELE.exe"}"#,
        );
        assert_eq!(
            ler(&bloqueio, Path::new(INSTALADO)),
            Entrada::Bloqueada {
                regra: "SEELE".to_owned()
            }
        );
    }

    #[test]
    fn o_bloqueio_vence_a_regra_que_libera() {
        let os_dois = com(&format!(
            "{}\n{}",
            r#"regra {"nome":"SEELE","ligada":"True","direcao":"Inbound","acao":"Allow","protocolo":"UDP","perfil":"Any","programa":"C:\\Program Files\\SEELE\\SEELE.exe"}"#,
            r#"regra {"nome":"SEELE","ligada":"True","direcao":"Inbound","acao":"Block","protocolo":"Any","perfil":"Any","programa":"c:\\program files\\seele\\seele.exe"}"#,
        ));
        assert!(matches!(
            ler(&os_dois, Path::new(INSTALADO)),
            Entrada::Bloqueada { .. }
        ));
    }

    #[test]
    fn regra_desligada_de_saida_ou_de_tcp_nao_libera() {
        for (campo, valor) in [
            ("ligada", "False"),
            ("direcao", "Outbound"),
            ("protocolo", "TCP"),
        ] {
            let mut regra = serde_json::json!({
                "nome": "SEELE", "ligada": "True", "direcao": "Inbound", "acao": "Allow",
                "protocolo": "UDP", "perfil": "Any", "programa": INSTALADO,
            });
            if let Some(lugar) = regra.get_mut(campo) {
                *lugar = serde_json::Value::String(valor.to_owned());
            }
            let saida = com(&format!("regra {regra}"));
            assert_eq!(
                ler(&saida, Path::new(INSTALADO)),
                Entrada::SemRegra,
                "uma regra com {campo} = {valor} passou por liberação"
            );
        }
    }

    #[test]
    fn o_caminho_conta_inteiro_e_sem_caixa() {
        // As regras do aviso do Windows guardam o caminho em minúsculas: o
        // mesmo programa, escrito com outra caixa, é o mesmo.
        let minusculas = DO_PC.replace(r"C:\\users\\aleta", r"C:\\Users\\Aleta");
        let dev = r"c:\USERS\aleta\AppData\Local\SEELE\seele-app.exe";
        assert!(matches!(
            ler(
                &minusculas.replace("rede Private", "rede Public"),
                Path::new(dev)
            ),
            Entrada::Liberada
        ));
        // E o mesmo nome noutra pasta **não** é o mesmo programa: a regra de
        // `C:\Program Files\SEELE\SEELE.exe` não libera um `D:\SEELE\SEELE.exe`.
        assert_eq!(
            ler(DO_PC, Path::new(r"D:\Programas\SEELE\SEELE.exe")),
            Entrada::OutraCopia {
                coberta: INSTALADO.to_owned()
            }
        );
    }

    #[test]
    fn uma_regra_so_do_perfil_publico_nao_vale_numa_rede_particular() {
        // O par do aviso do Windows no PC de testes: `Public`, e a rede da
        // casa é `Private`.
        let dev = r"C:\users\aleta\appdata\local\seele\seele-app.exe";
        assert_eq!(
            ler(DO_PC, Path::new(dev)),
            Entrada::SoNoPerfil {
                perfis: "Public".to_owned(),
                redes: "Private".to_owned()
            }
        );
        assert_eq!(
            ler(
                &DO_PC.replace("rede Private", "rede Public"),
                Path::new(dev)
            ),
            Entrada::Liberada
        );
    }

    #[test]
    fn sem_regra_nenhuma_e_sem_regra() {
        assert_eq!(
            ler("rede Private\nfim\n", Path::new(INSTALADO)),
            Entrada::SemRegra
        );
    }

    #[test]
    fn uma_leitura_que_nao_se_entende_nunca_vira_acusacao() {
        for saida in [
            "",
            "Get-NetFirewallApplicationFilter : Acesso negado.",
            "rede Private\n",
            "regra {nao e json}\nfim\n",
        ] {
            let entrada = ler(saida, Path::new(INSTALADO));
            assert!(
                matches!(entrada, Entrada::NaoSei { .. }),
                "«{saida}» virou {entrada:?}, e não «não sei»"
            );
            assert_eq!(frase(&entrada, Path::new(INSTALADO)), None);
        }
    }

    #[test]
    fn as_frases_dizem_o_que_a_regra_e_e_nao_uma_porta() {
        let exe = Path::new(INSTALADO);
        for entrada in [
            Entrada::SemRegra,
            Entrada::Bloqueada {
                regra: "SEELE".to_owned(),
            },
            Entrada::SoNoPerfil {
                perfis: "Public".to_owned(),
                redes: "Private".to_owned(),
            },
            Entrada::OutraCopia {
                coberta: r"C:\Program Files\SEELE\SEELE.exe".to_owned(),
            },
        ] {
            let Some(texto) = frase(&entrada, exe) else {
                panic!("{entrada:?} não disse nada");
            };
            assert!(texto.contains("firewall do Windows"), "{texto}");
            for porta in ["8383", "8384", "8385", "porta"] {
                assert!(
                    !texto.contains(porta),
                    "a frase de {entrada:?} fala de porta: {texto}"
                );
            }
        }
        assert_eq!(frase(&Entrada::Liberada, exe), None);
        assert!(frase(
            &Entrada::SoNoPerfil {
                perfis: "Public".to_owned(),
                redes: "Private".to_owned()
            },
            exe
        )
        .is_some_and(|texto| texto.contains("pública") && texto.contains("particular")));
    }

    #[test]
    fn o_comando_codificado_e_o_base64_do_utf16() {
        use base64::Engine as _;
        for texto in ["", "a", "ab", "abc", "fim", SCRIPT, "ação — «aspas»"] {
            let bytes: Vec<u8> = texto.encode_utf16().flat_map(u16::to_le_bytes).collect();
            assert_eq!(
                comando_codificado(texto),
                base64::engine::general_purpose::STANDARD.encode(bytes),
                "«{texto}»"
            );
        }
    }

    /// **No Windows, a leitura de verdade chega ao fim**: o PowerShell abre, o
    /// script roda inteiro e a saída se lê. Qualquer coisa menos «não sei» —
    /// numa máquina de CI, sem o SEELE instalado, o certo é «sem regra». Só
    /// roda no Windows (o CI e o PC de testes); fora dele, o teste de baixo
    /// diz isso pelo nome.
    #[cfg(windows)]
    #[test]
    fn no_windows_a_leitura_de_verdade_chega_ao_fim() {
        let entrada = ler_deste_programa();
        assert!(
            !matches!(entrada, Entrada::NaoSei { .. }),
            "a leitura do firewall do Windows não se completou: {entrada:?}"
        );
    }

    /// Fora do Windows a leitura de verdade não roda, e isto aparece no
    /// relatório dizendo-o — um `cfg` mudo teria a mesma cor de passar.
    #[cfg(not(windows))]
    #[test]
    fn fora_do_windows_a_leitura_de_verdade_nao_roda() {
        eprintln!(
            "PARCIAL: a leitura do firewall pelo PowerShell só roda no Windows \
             (`no_windows_a_leitura_de_verdade_chega_ao_fim`); aqui só a decisão é provada."
        );
    }

    #[test]
    fn o_script_so_usa_aspas_simples_e_termina_em_fim() {
        // Aspas duplas no script sobreviveriam ao `-EncodedCommand`, mas é por
        // causa dele que elas não importam: este guarda prende o `fim`, sem o
        // qual a leitura é sempre «não sei».
        assert!(SCRIPT.trim_end().ends_with("'fim'"));
        assert!(SCRIPT.contains("[Console]::OutputEncoding = [Text.Encoding]::UTF8"));
        assert!(SCRIPT.contains("-PolicyStore ActiveStore"));
    }
}
