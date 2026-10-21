//! **Os MODs da sessão no navegador** (Tarefa 2.6 da F4): o que os comandos do
//! desktop decidem sobre um pacote, para a página que guarda os pacotes no
//! armazenamento dela (o IndexedDB) e os baixa do catálogo pelo `fetch`.
//!
//! As regras são as do desktop, pelas mesmas funções: o pacote lido dos bytes
//! e conferido contra o hash pelo qual foi guardado
//! (`seele_sessao::mods`), o arquivo que um MOD serve
//! (`caminho_declarado`), e o catálogo conferido, lido, planejado e recusado
//! (`seele_sessao::catalogo`). A página só move bytes: lê o que guardou, baixa
//! o que o plano diz, e guarda o que a conferência aprovou.

use seele_sessao::catalogo::{self, FalhaNoCatalogo};
use seele_sessao::mods::{self as pacote, ModInstalado};
use serde_json::{json, Value};

/// A chave pública do catálogo: o mesmo arquivo que o desktop compila
/// (`apps/seele-app/chaves/mods.pub`), compilado para dentro do `.wasm`.
pub const CHAVE_DO_CATALOGO: &str = include_str!("../../../apps/seele-app/chaves/mods.pub");

/// **Se a exigência de um MOD ligado tranca alguém na porta**: o
/// `exigencia_vale_na_rede` do `seele-server`, que o desktop manda à tela ao
/// lado do interruptor de quem hospeda. O navegador não hospeda, e o campo vai
/// na lista do mesmo jeito; a conformidade (`dois_produtores_dos_mods.rs`) o
/// prende ao do servidor.
pub const EXIGENCIA_VALE_NA_REDE: bool = true;

pub use seele_sessao::mods::Guardado;

/// **O `mods_instalados` do desktop**: cada pacote guardado como a janela o
/// desenha (o `ModNaTela`), na ordem da lista do desktop. Nenhum está ligado
/// e nenhum pinta: o navegador não hospeda.
#[must_use]
pub fn na_tela(guardados: Vec<Guardado>) -> Value {
    Value::Array(
        pacote::listar_guardados(guardados)
            .into_iter()
            .map(|instalado| {
                let mut linha = serde_json::to_value(instalado).unwrap_or(Value::Null);
                if let Value::Object(campos) = &mut linha {
                    campos.insert("enabled".into(), json!(false));
                    campos.insert("pinta".into(), json!(false));
                    campos.insert(
                        "exigencia_vale_na_rede".into(),
                        json!(EXIGENCIA_VALE_NA_REDE),
                    );
                }
                linha
            })
            .collect(),
    )
}

/// O arquivo que um MOD serve, dos arquivos guardados: o caminho declarado
/// (`caminho_declarado`), e o arquivo guardado sob o mesmo caminho,
/// comparado por componentes como o disco do desktop compara.
#[must_use]
pub fn servir<'a>(
    arquivos: &'a [(String, Vec<u8>)],
    instalado: &ModInstalado,
    url_path: &str,
) -> Option<&'a [u8]> {
    let relativo = pacote::caminho_declarado(instalado, url_path)?;
    arquivos
        .iter()
        .find(|(caminho, _)| std::path::Path::new(caminho) == relativo.as_path())
        .map(|(_, bytes)| bytes.as_slice())
}

/// O pacote pedido por `hash` e de `id`, dos arquivos que a página leu: o
/// `ler_por_hash` do desktop e a conferência do dono.
///
/// # Errors
///
/// O motivo do desktop: o nome da recusa do manifesto, `conteudo-nao-bate` e
/// `conteudo-de-outro-mod`.
pub fn do_mod(
    arquivos: &mut [(String, Vec<u8>)],
    id: &str,
    hash: &str,
) -> Result<ModInstalado, String> {
    let instalado = pacote::pelo_hash(pacote::ler_pacote(arquivos), hash)?;
    // **O hash tem de ser deste MOD.** Sem isto, uma janela que pedisse o
    // código de um MOD sob o nome de outro receberia o primeiro.
    if instalado.id != id {
        return Err("conteudo-de-outro-mod".to_owned());
    }
    Ok(instalado)
}

/// **O `codigo_do_mod`**: o fonte do cliente e as capacidades da API que o
/// pacote declara, que é o que o executor do navegador recebe
/// (`mods-no-navegador.js`). O desktop devolve `(fonte, api)` e o executor
/// nativo pega as capacidades por outra porta; a regra das capacidades é a
/// mesma (`capacidades_da_api`), e a ponte tira as que a plataforma não faz.
///
/// # Errors
///
/// O motivo do desktop, na ordem dele: os de [`do_mod`],
/// `sem-metade-de-cliente`, `cliente-nao-servido` e `cliente-nao-e-utf8`.
pub fn codigo(arquivos: &mut [(String, Vec<u8>)], id: &str, hash: &str) -> Result<Value, String> {
    let instalado = do_mod(arquivos, id, hash)?;
    let Some(cliente) = instalado.client.clone() else {
        return Err("sem-metade-de-cliente".to_owned());
    };
    let bytes = servir(arquivos, &instalado, &format!("/{id}/{cliente}"))
        .ok_or_else(|| "cliente-nao-servido".to_owned())?;
    let fonte = std::str::from_utf8(bytes).map_err(|_| "cliente-nao-e-utf8".to_owned())?;
    Ok(json!({
        "fonte": fonte,
        "capacidades": pacote::capacidades_da_api(instalado.api),
    }))
}

/// O que a página baixou do catálogo, com as assinaturas.
pub struct Baixado<'a> {
    /// O `catalogo.json`, cru.
    pub catalogo: &'a [u8],
    /// O `catalogo.json.minisig`.
    pub assinatura: &'a str,
    /// O `revogacoes.json`, cru.
    pub revogacoes: &'a [u8],
    /// O `revogacoes.json.minisig`.
    pub assinatura_das_revogacoes: &'a str,
}

/// **O plano do `instalar_mod_do_catalogo`**: o catálogo e as revogações
/// conferidos contra `chave` e lidos, e os arquivos a baixar, com o hash que o
/// conjunto tem de dar. A lista de revogações que não passa **não** vira lista
/// vazia, como no desktop.
///
/// # Errors
///
/// A [`FalhaNoCatalogo`] do desktop.
pub fn planejar(
    baixado: &Baixado<'_>,
    chave: &str,
    id: &str,
    versao: &str,
) -> Result<Value, FalhaNoCatalogo> {
    let lido = catalogo::ler_catalogo(baixado.catalogo, baixado.assinatura, chave)?;
    let revogacoes =
        catalogo::ler_revogacoes(baixado.revogacoes, baixado.assinatura_das_revogacoes, chave)?;
    let plano = catalogo::planejar(&lido, &revogacoes, id, versao)?;
    Ok(json!({ "arquivos": plano.arquivos, "hash": plano.hash }))
}

/// **Os bytes baixados**: o conjunto conferido contra o que o catálogo
/// declarou, e o pacote que serve a este SEELE. O `id` e o hash sob o qual a
/// página o guarda.
///
/// # Errors
///
/// [`FalhaNoCatalogo::ConteudoDivergente`], e a recusa do pacote como
/// [`FalhaNoCatalogo::NaoInstalou`], como o desktop.
pub fn conferir_baixados(
    baixados: &mut [(String, Vec<u8>)],
    esperado: &str,
) -> Result<Value, FalhaNoCatalogo> {
    catalogo::conferir_conjunto(baixados, esperado)?;
    let (id, hash) = catalogo::pacote_que_serve(baixados).map_err(FalhaNoCatalogo::NaoInstalou)?;
    Ok(json!({ "id": id, "hash": hash }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    const MANIFESTO: &str = r#"{"schema":1,"id":"ana/mesa","version":"1.0.0","api":6,"repo":"https://exemplo.org/mesa","client":"cliente/main.js","arquivos":["som/toque.wav"]}"#;

    fn arquivos() -> Vec<(String, Vec<u8>)> {
        vec![
            ("mod.json".to_owned(), MANIFESTO.as_bytes().to_vec()),
            ("cliente/main.js".to_owned(), b"SeeleUI.pronto();".to_vec()),
            ("som/toque.wav".to_owned(), b"RIFF".to_vec()),
            ("leia.txt".to_owned(), b"nao declarado".to_vec()),
        ]
    }

    #[test]
    fn o_codigo_sai_com_as_capacidades_da_api_e_as_recusas_sao_as_do_desktop() {
        let hash = pacote::hash_do_conjunto(&mut arquivos());
        let lido = codigo(&mut arquivos(), "ana/mesa", &hash).unwrap();
        assert_eq!(lido["fonte"], "SeeleUI.pronto();");
        assert!(lido["capacidades"]
            .as_array()
            .unwrap()
            .contains(&json!("volume")));

        assert_eq!(
            codigo(&mut arquivos(), "ana/outro", &hash),
            Err("conteudo-de-outro-mod".to_owned())
        );
        assert_eq!(
            codigo(&mut arquivos(), "ana/mesa", "0".repeat(64).as_str()),
            Err("conteudo-nao-bate".to_owned())
        );
        let mut sem_cliente = arquivos();
        sem_cliente[0].1 = MANIFESTO
            .replace(r#""client":"cliente/main.js","#, r#""server":"s.js","#)
            .into_bytes();
        let hash_sem = pacote::hash_do_conjunto(&mut sem_cliente);
        assert_eq!(
            codigo(&mut sem_cliente, "ana/mesa", &hash_sem),
            Err("sem-metade-de-cliente".to_owned())
        );
        let mut torto = arquivos();
        torto[1].1 = vec![0xff, 0xfe];
        let hash_torto = pacote::hash_do_conjunto(&mut torto);
        assert_eq!(
            codigo(&mut torto, "ana/mesa", &hash_torto),
            Err("cliente-nao-e-utf8".to_owned())
        );
    }

    #[test]
    fn so_o_declarado_e_servido_e_nada_sobe_para_fora() {
        let lugar = pacote::hash_do_conjunto(&mut arquivos());
        let guardados = arquivos();
        let instalado = pacote::guardado_em(guardados.clone(), &lugar);
        assert_eq!(
            servir(&guardados, &instalado, "/ana/mesa/som/toque.wav"),
            Some(&b"RIFF"[..])
        );
        // O pedaço vazio é recusado, como no `inner_path` do desktop.
        for fora in [
            "/ana/mesa/som//toque.wav",
            "/ana/mesa/leia.txt",
            "/ana/mesa/../mesa/mod.json",
            "/ana/outro/som/toque.wav",
            "/ana/mesa/",
            "",
        ] {
            assert_eq!(servir(&guardados, &instalado, fora), None, "{fora}");
        }
    }

    #[test]
    fn a_lista_diz_cada_guardado_e_o_que_o_navegador_nao_liga() {
        let lugar = pacote::hash_do_conjunto(&mut arquivos());
        let lista = na_tela(vec![
            (lugar.clone(), arquivos()),
            ("f".repeat(64), arquivos()),
        ]);
        assert_eq!(lista[0]["refused"], Value::Null);
        assert_eq!(lista[1]["refused"], "malformed-id");
        assert_eq!(
            (lista[0]["enabled"].clone(), lista[0]["pinta"].clone()),
            (json!(false), json!(false))
        );
        assert_eq!(lista[0]["exigencia_vale_na_rede"], json!(true));
    }

    #[test]
    fn o_baixado_que_diverge_ou_nao_serve_e_recusado_com_o_nome_do_desktop() {
        let hash = pacote::hash_do_conjunto(&mut arquivos());
        assert_eq!(
            conferir_baixados(&mut arquivos(), &hash).unwrap(),
            json!({ "id": "ana/mesa", "hash": hash })
        );
        assert!(matches!(
            conferir_baixados(&mut arquivos(), &"0".repeat(64)),
            Err(FalhaNoCatalogo::ConteudoDivergente { .. })
        ));
        let mut sem = vec![("cliente/main.js".to_owned(), Vec::new())];
        let hash_sem = pacote::hash_do_conjunto(&mut sem);
        assert_eq!(
            conferir_baixados(&mut sem, &hash_sem),
            Err(FalhaNoCatalogo::NaoInstalou(
                catalogo::FalhaAoInstalarMod::SemManifesto
            ))
        );
    }

    #[test]
    fn o_catalogo_do_indexador_passa_com_a_chave_embutida() {
        let baixado = Baixado {
            catalogo: include_bytes!("../../../apps/seele-app/testes/catalogo-do-indexador.json"),
            assinatura: include_str!(
                "../../../apps/seele-app/testes/catalogo-do-indexador.json.minisig"
            ),
            revogacoes: include_bytes!(
                "../../../apps/seele-app/testes/revogacoes-do-indexador.json"
            ),
            assinatura_das_revogacoes: include_str!(
                "../../../apps/seele-app/testes/revogacoes-do-indexador.json.minisig"
            ),
        };
        assert!(matches!(
            planejar(&baixado, CHAVE_DO_CATALOGO, "ninguem/nada", "1"),
            Err(FalhaNoCatalogo::NaoEstaNoCatalogo(_))
        ));
        let mesa = serde_json::from_slice::<Value>(baixado.catalogo).unwrap()["mods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "seele/mesa")
            .unwrap()["versoes"][0]["versao"]
            .as_str()
            .unwrap()
            .to_owned();
        let plano = planejar(&baixado, CHAVE_DO_CATALOGO, "seele/mesa", &mesa).unwrap();
        assert!(plano["arquivos"][0][1]
            .as_str()
            .unwrap()
            .starts_with(&format!("{}/mods/seele/mesa/{mesa}/", catalogo::ENDERECO)));
        // A assinatura que não é desta chave não passa, e nada é lido.
        let outro = Baixado {
            catalogo: br#"{"esquema":1,"mods":[]}"#,
            ..baixado
        };
        assert_eq!(
            planejar(&outro, CHAVE_DO_CATALOGO, "seele/mesa", &mesa),
            Err(FalhaNoCatalogo::AssinaturaDoCatalogoRecusada)
        );
    }
}
