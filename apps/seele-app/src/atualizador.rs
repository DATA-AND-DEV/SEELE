//! **O aviso antes de atualizar para outra linha** (E4 da 0.17; ADR 0063).
//!
//! O botão de atualizar troca este app pela versão do topo do `latest.json`.
//! Quando essa versão fala outra linha de servidor — a 0.16 (Servidor 1) indo
//! para a 0.17 (Servidor 2), ou a 0.17 indo para uma futura linha 3 —, a troca
//! não é só um número: quem atualiza deixa de entrar nos servidores da linha
//! de antes, e quem ficou nela deixa de entrar nos desta. Isso tem de ser dito
//! **antes** de instalar, e não depois, com três saídas: cancelar, adiar ou
//! prosseguir.
//!
//! Este módulo é a decisão, sem rede e sem Tauri, para poder ser provada por
//! teste. O `main.rs` só a chama: `procurar_atualizacao` põe a [`Fronteira`] na
//! resposta, e `instalar_atualizacao` recusa uma travessia que não foi
//! confirmada para **aquela** versão ([`pode_instalar`]).
//!
//! # De onde vem a linha da versão nova
//!
//! O `latest.json` é um arquivo só para o atualizador e para o launcher
//! (`empacotar/manifesto.py`). O topo diz `version`, `notes` e `platforms`; a
//! lista `versions[]` diz, de cada versão, a `unit.server` («2.0», «1.0»). A
//! linha da nova sai de lá, pelo mesmo `Linha::do_servidor` do launcher. Sem a
//! entrada, ou com um `server` ilegível, a resposta é [`Fronteira::NaoSabe`] —
//! e não sabe **avisa**: um aviso a mais custa um clique, e um a menos custa a
//! pessoa descobrir pela sala vazia.
//!
//! # O que este módulo não alcança
//!
//! Ele roda na 0.17 em diante. **Quem está na 0.16 não tem este código**: o que
//! uma 0.16 mostra antes de instalar a 0.17 é só o campo `notes` do
//! `latest.json`, no bloco «O QUE MUDA» da configuração. O que dá para fazer por
//! esse lado sem tocar na 0.16 está em `docs/versoes-lado-a-lado.md`.

use std::path::Path;

use seele_lancador::Linha;

/// A relação entre a linha desta versão e a da versão oferecida.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub(crate) enum Fronteira {
    /// A mesma linha: atualizar não muda com quem se conversa. Sem aviso.
    Mesma {
        /// A linha, no texto canônico («2»).
        linha: String,
    },
    /// Outra linha: quem atualiza sai dos servidores de `de`, e entra nos de
    /// `para`. Aviso antes de instalar.
    Cruza {
        /// A linha desta versão.
        de: String,
        /// A linha da versão oferecida.
        para: String,
    },
    /// O manifesto não diz a linha da versão oferecida, ou esta versão não
    /// sabe a dela. Aviso, com a frase de quem não sabe.
    NaoSabe,
}

impl Fronteira {
    /// Se instalar exige a confirmação da pessoa.
    #[must_use]
    pub(crate) const fn pede_aviso(&self) -> bool {
        !matches!(self, Self::Mesma { .. })
    }
}

/// A fronteira entre `instalada` e `nova`.
#[must_use]
pub(crate) fn fronteira(instalada: Option<&Linha>, nova: Option<&Linha>) -> Fronteira {
    match (instalada, nova) {
        (Some(de), Some(para)) if de == para => Fronteira::Mesma {
            linha: de.como_texto(),
        },
        (Some(de), Some(para)) => Fronteira::Cruza {
            de: de.como_texto(),
            para: para.como_texto(),
        },
        _ => Fronteira::NaoSabe,
    }
}

/// A linha que o manifesto atribui a `versao`: a `unit.server` da entrada dela
/// em `versions[]`.
#[must_use]
pub(crate) fn linha_no_manifesto(manifesto: &serde_json::Value, versao: &str) -> Option<Linha> {
    manifesto
        .get("versions")?
        .as_array()?
        .iter()
        .find(|entrada| entrada.get("version").and_then(|v| v.as_str()) == Some(versao))?
        .get("unit")?
        .get("server")?
        .as_str()
        .and_then(|servidor| Linha::do_servidor(servidor).ok())
}

/// A fronteira de uma oferta do atualizador, pelo manifesto que veio com ela.
#[must_use]
pub(crate) fn fronteira_da_oferta(
    instalada: Option<&Linha>,
    manifesto: &serde_json::Value,
    versao_nova: &str,
) -> Fronteira {
    fronteira(
        instalada,
        linha_no_manifesto(manifesto, versao_nova).as_ref(),
    )
}

/// O que a pessoa escolheu diante do aviso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Escolha {
    /// Não atualizar agora, e esquecer a oferta.
    Cancelar,
    /// Não atualizar agora, e lembrar que esta versão foi adiada: a próxima
    /// procura a mostra de novo, marcada como adiada, e ainda com o aviso.
    Adiar,
    /// Atualizar, sabendo da travessia.
    Prosseguir,
}

/// Por que a instalação foi recusada antes de baixar um byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Recusa {
    /// A versão cruza a fronteira e a confirmação não veio, ou veio para outra
    /// versão.
    TravessiaNaoConfirmada,
    /// A pessoa cancelou ou adiou.
    NaoQuis,
}

/// Se a oferta `versao`, com esta `fronteira`, pode ser instalada pela
/// `escolha` e pela `confirmada` que a tela mandou.
///
/// - na mesma linha, basta não ter cancelado nem adiado;
/// - numa travessia, a tela tem de mandar `Prosseguir` **e** a versão que ela
///   mostrou no aviso: uma confirmação velha, de outra oferta, não serve.
///
/// # Errors
///
/// [`Recusa`], que diz qual das duas regras faltou.
pub(crate) fn pode_instalar(
    fronteira: &Fronteira,
    versao: &str,
    escolha: Escolha,
    confirmada: Option<&str>,
) -> Result<(), Recusa> {
    match escolha {
        Escolha::Cancelar | Escolha::Adiar => Err(Recusa::NaoQuis),
        Escolha::Prosseguir if !fronteira.pede_aviso() => Ok(()),
        Escolha::Prosseguir if confirmada == Some(versao) => Ok(()),
        Escolha::Prosseguir => Err(Recusa::TravessiaNaoConfirmada),
    }
}

/// O nome do arquivo do adiamento, ao lado das preferências.
pub(crate) const ARQUIVO_DO_ADIAMENTO: &str = "atualizacao-adiada";

/// Guarda que `versao` foi adiada.
///
/// # Errors
///
/// O erro do sistema ao gravar.
pub(crate) fn adiar(config: &Path, versao: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(config)?;
    std::fs::write(config.join(ARQUIVO_DO_ADIAMENTO), format!("{versao}\n"))
}

/// Esquece o adiamento, se houver.
pub(crate) fn esquecer_o_adiamento(config: &Path) {
    let _ = std::fs::remove_file(config.join(ARQUIVO_DO_ADIAMENTO));
}

/// Se `versao` é a que foi adiada.
#[must_use]
pub(crate) fn foi_adiada(config: &Path, versao: &str) -> bool {
    std::fs::read_to_string(config.join(ARQUIVO_DO_ADIAMENTO))
        .is_ok_and(|guardada| guardada.trim() == versao)
}

/// O que fazer com a escolha, do lado do disco: adiar grava, cancelar e
/// prosseguir esquecem o adiamento.
///
/// # Errors
///
/// O erro do sistema ao gravar o adiamento.
pub(crate) fn registrar_a_escolha(
    config: &Path,
    versao: &str,
    escolha: Escolha,
) -> std::io::Result<()> {
    match escolha {
        Escolha::Adiar => adiar(config, versao),
        Escolha::Cancelar | Escolha::Prosseguir => {
            esquecer_o_adiamento(config);
            Ok(())
        }
    }
}

#[cfg(test)]
mod testes {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn linha(texto: &str) -> Linha {
        Linha::nova(texto).unwrap()
    }

    fn manifesto(entradas: &[(&str, &str)]) -> serde_json::Value {
        let topo = entradas.first().map_or("", |(versao, _)| *versao);
        serde_json::json!({
            "version": topo,
            "notes": "SEELE",
            "versions": entradas
                .iter()
                .map(|(versao, servidor)| serde_json::json!({
                    "version": versao,
                    "unit": { "protocol": 10, "mod_api": 6, "server": servidor },
                }))
                .collect::<Vec<_>>(),
        })
    }

    /// **Legado → nova avisa**: a 0.16 (linha 1) indo para a 0.17 (linha 2).
    #[test]
    fn do_legado_para_a_nova_avisa() {
        let m = manifesto(&[("0.17.0", "2.0"), ("0.16.0", "1.0")]);
        let f = fronteira_da_oferta(Some(&linha("1")), &m, "0.17.0");
        assert_eq!(
            f,
            Fronteira::Cruza {
                de: "1".into(),
                para: "2".into()
            }
        );
        assert!(f.pede_aviso());
    }

    /// **Nova → nova não avisa**: a 0.17.0 indo para a 0.17.1, as duas na 2.
    #[test]
    fn da_nova_para_a_nova_nao_avisa() {
        let m = manifesto(&[("0.17.1", "2.0"), ("0.17.0", "2.0")]);
        let f = fronteira_da_oferta(Some(&linha("2")), &m, "0.17.1");
        assert_eq!(f, Fronteira::Mesma { linha: "2".into() });
        assert!(!f.pede_aviso());
        assert_eq!(
            pode_instalar(&f, "0.17.1", Escolha::Prosseguir, None),
            Ok(())
        );
    }

    /// Uma futura linha 3 avisa a 0.17 do mesmo jeito.
    #[test]
    fn a_proxima_quebra_tambem_avisa() {
        let m = manifesto(&[("0.20.0", "3.0"), ("0.17.0", "2.0")]);
        assert!(fronteira_da_oferta(Some(&linha("2")), &m, "0.20.0").pede_aviso());
    }

    /// Sem a entrada da versão nova, ou com um `server` ilegível, não se sabe —
    /// e não saber avisa.
    #[test]
    fn sem_a_linha_no_manifesto_avisa() {
        let m = manifesto(&[("0.17.0", "2.0")]);
        assert_eq!(
            fronteira_da_oferta(Some(&linha("2")), &m, "0.18.0"),
            Fronteira::NaoSabe
        );
        let torto = manifesto(&[("0.18.0", "dois")]);
        assert_eq!(
            fronteira_da_oferta(Some(&linha("2")), &torto, "0.18.0"),
            Fronteira::NaoSabe
        );
        assert_eq!(fronteira_da_oferta(None, &m, "0.17.0"), Fronteira::NaoSabe);
        assert!(Fronteira::NaoSabe.pede_aviso());
        assert_eq!(
            fronteira_da_oferta(Some(&linha("2")), &serde_json::json!({}), "0.17.0"),
            Fronteira::NaoSabe
        );
    }

    /// **Cancelar não instala**, nem com a confirmação na mão.
    #[test]
    fn cancelar_nao_instala() {
        let f = fronteira(Some(&linha("1")), Some(&linha("2")));
        assert_eq!(
            pode_instalar(&f, "0.17.0", Escolha::Cancelar, Some("0.17.0")),
            Err(Recusa::NaoQuis)
        );
        let mesma = fronteira(Some(&linha("2")), Some(&linha("2")));
        assert_eq!(
            pode_instalar(&mesma, "0.17.1", Escolha::Cancelar, None),
            Err(Recusa::NaoQuis)
        );
    }

    /// **Prosseguir** instala uma travessia só com a confirmação **desta**
    /// versão. Uma confirmação de outra oferta, ou nenhuma, recusa.
    #[test]
    fn prosseguir_pede_a_confirmacao_desta_versao() {
        let f = fronteira(Some(&linha("1")), Some(&linha("2")));
        assert_eq!(
            pode_instalar(&f, "0.17.0", Escolha::Prosseguir, Some("0.17.0")),
            Ok(())
        );
        assert_eq!(
            pode_instalar(&f, "0.17.0", Escolha::Prosseguir, None),
            Err(Recusa::TravessiaNaoConfirmada)
        );
        assert_eq!(
            pode_instalar(&f, "0.17.1", Escolha::Prosseguir, Some("0.17.0")),
            Err(Recusa::TravessiaNaoConfirmada),
            "uma confirmação velha passou por uma oferta nova"
        );
    }

    /// **Adiar** não instala, grava a versão adiada, e a próxima procura sabe
    /// que foi ela. Uma versão mais nova não herda o adiamento; prosseguir e
    /// cancelar o esquecem.
    #[test]
    fn adiar_nao_instala_e_lembra_a_versao() {
        let dir = tempfile::tempdir().unwrap();
        let f = fronteira(Some(&linha("1")), Some(&linha("2")));
        assert_eq!(
            pode_instalar(&f, "0.17.0", Escolha::Adiar, Some("0.17.0")),
            Err(Recusa::NaoQuis)
        );
        registrar_a_escolha(dir.path(), "0.17.0", Escolha::Adiar).unwrap();
        assert!(foi_adiada(dir.path(), "0.17.0"));
        assert!(!foi_adiada(dir.path(), "0.17.1"));
        registrar_a_escolha(dir.path(), "0.17.0", Escolha::Prosseguir).unwrap();
        assert!(!foi_adiada(dir.path(), "0.17.0"));
        registrar_a_escolha(dir.path(), "0.17.0", Escolha::Adiar).unwrap();
        registrar_a_escolha(dir.path(), "0.17.0", Escolha::Cancelar).unwrap();
        assert!(!foi_adiada(dir.path(), "0.17.0"));
    }

    /// A forma que a tela lê: `{"tipo": "cruza", "de": "1", "para": "2"}`.
    #[test]
    fn a_fronteira_vai_para_a_tela_com_o_tipo() {
        assert_eq!(
            serde_json::to_value(fronteira(Some(&linha("1")), Some(&linha("2")))).unwrap(),
            serde_json::json!({ "tipo": "cruza", "de": "1", "para": "2" })
        );
        assert_eq!(
            serde_json::to_value(Fronteira::NaoSabe).unwrap(),
            serde_json::json!({ "tipo": "nao_sabe" })
        );
        let escolha: Escolha = serde_json::from_str("\"adiar\"").unwrap();
        assert_eq!(escolha, Escolha::Adiar);
    }
}
