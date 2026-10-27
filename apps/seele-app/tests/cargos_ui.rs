//! Os guardas da tela de cargos e permissões (E6 da 0.17, cargos-ui).
//!
//! A bancada `bancada/cargos.cjs` prova o comportamento num navegador de
//! verdade. Estes guardas provam o que a bancada não vê, lendo os arquivos:
//!
//! - que toda permissão do produto (o `Permission` de verdade, lido do
//!   `seele-proto`) tem nome, descrição e grupo na tela — uma permissão nova
//!   sem frase apareceria como «desconhecida» para quem administra;
//! - que a tela não decide quem pode: nenhum nome de permissão é comparado no
//!   `cargos.js`, e todo pedido passa por `cargosPedir`, que aplica a resposta
//!   à sessão de origem;
//! - que o estado de cargos não vaza entre sessões: nada de estado solto no
//!   topo do arquivo além do `Map` por sessão e dos quatro ponteiros da camada;
//! - que a folha segue a estética: só tokens, nada do vermelho de alerta, e
//!   toda classe com o prefixo da camada;
//! - que a camada é um diálogo acessível, com as abas ligadas aos painéis.
//!
//! O comportamento que estes guardas protegem está provado por reversão na
//! bancada (`--provar-reversoes`, `docs/evidencias/ui-0.17/cargos/LEIA.md`).
//! A reversão DESTES guardas — desfazer à mão e ver o teste ficar vermelho —
//! ainda não foi executada: na hora em que nasceram, rodar o binário pedia
//! aprovação, e o `cargo test` integrado não compilava por outro crate.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::path::PathBuf;

/// `apps/seele-app/`. Pelo Cargo, a pasta do pacote; compilado à parte
/// (`rustc --test` sobre o caminho absoluto deste arquivo, como se fez quando
/// o resto do workspace não compilava), a pasta acima de `tests/`.
fn app_dir() -> PathBuf {
    option_env!("CARGO_MANIFEST_DIR").map_or_else(
        || {
            PathBuf::from(file!())
                .parent()
                .and_then(std::path::Path::parent)
                .expect("este arquivo mora em apps/seele-app/tests")
                .to_path_buf()
        },
        PathBuf::from,
    )
}

/// Um arquivo a partir de `apps/seele-app/`, com as quebras normalizadas.
fn read(relative: &str) -> String {
    std::fs::read_to_string(app_dir().join(relative))
        .unwrap_or_else(|error| panic!("{relative}: {error}"))
        .replace("\r\n", "\n")
}

/// O texto sem comentários de linha e de bloco: uma explicação que cita um
/// nome não pode satisfazer nem acusar um guarda.
fn sem_comentarios(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut resto = texto;
    while !resto.is_empty() {
        if let Some(depois) = resto.strip_prefix("/*") {
            resto = depois.split_once("*/").map_or("", |(_, d)| d);
        } else if resto.starts_with("//") && !saida.ends_with(':') {
            resto = resto.split_once('\n').map_or("", |(_, d)| d);
            saida.push('\n');
        } else {
            let mut letras = resto.chars();
            if let Some(letra) = letras.next() {
                saida.push(letra);
            }
            resto = letras.as_str();
        }
    }
    saida
}

/// As variantes de `seele_proto::control::Permission`, lidas do fonte.
fn variantes_de_permission() -> Vec<String> {
    let fonte = read("../../crates/seele-proto/src/control.rs");
    let Some(corpo) = fonte.split("pub enum Permission {").nth(1) else {
        panic!("`pub enum Permission` sumiu de control.rs, e este guarda ficou cego");
    };
    let corpo = corpo.split("\n}").next().unwrap_or_default();
    let variantes: Vec<String> = sem_comentarios(corpo)
        .lines()
        .map(str::trim)
        .filter(|linha| !linha.is_empty() && !linha.starts_with('#'))
        .filter_map(|linha| linha.strip_suffix(','))
        .filter(|nome| nome.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(str::to_owned)
        .collect();
    assert!(
        variantes.len() >= 13,
        "só {} variantes de Permission foram lidas: a leitura parou de casar e o guarda mede nada",
        variantes.len()
    );
    variantes
}

/// O corpo de uma entrada de `FRASES_DOS_CARGOS`, do nome até o `}),` dela.
fn entrada_das_frases(nome: &str) -> String {
    let frases = read("ui/frases.js");
    let Some(bloco) = frases
        .split("const FRASES_DOS_CARGOS = Object.freeze({")
        .nth(1)
    else {
        panic!("`FRASES_DOS_CARGOS` sumiu de frases.js");
    };
    let Some(entrada) = bloco.split(&format!("\n  {nome}: Object.freeze({{")).nth(1) else {
        panic!("`FRASES_DOS_CARGOS.{nome}` sumiu de frases.js");
    };
    entrada
        .split("\n  }),")
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn toda_permissao_do_produto_tem_nome_descricao_e_grupo_na_tela() {
    // Uma variante nova no fim do `Permission` (o fio só acrescenta) sem frase
    // aqui aparece para quem administra como «esta versão do app não conhece
    // esta permissão», e não se concede pela tela. O certo é ela chegar com o
    // nome e o que ela permite.
    let permissoes = entrada_das_frases("permissoes");
    let grupos = entrada_das_frases("grupoDe");
    let mut faltam = Vec::new();
    for variante in variantes_de_permission() {
        let com_frase = permissoes.contains(&format!("\n    {variante}: ["));
        let com_grupo = grupos.contains(&format!("\n    {variante}: \""));
        if !com_frase || !com_grupo {
            faltam.push(format!(
                "{variante}{}{}",
                if com_frase {
                    ""
                } else {
                    " (sem nome e descrição)"
                },
                if com_grupo { "" } else { " (sem grupo)" }
            ));
        }
    }
    assert!(
        faltam.is_empty(),
        "permissões do produto sem frase na tela de cargos: {faltam:?}"
    );
}

#[test]
fn os_grupos_de_permissao_sao_os_que_a_tela_desenha() {
    // `grupoDe` só pode apontar para um grupo que tem título e lugar na ordem;
    // um grupo sem título deixaria a permissão fora de toda seção.
    let grupos = entrada_das_frases("grupoDe");
    let titulos = entrada_das_frases("grupos");
    let script = read("ui/cargos.js");
    let ordem = script
        .split("const CARGOS_GRUPOS = Object.freeze([")
        .nth(1)
        .and_then(|d| d.split("]);").next())
        .expect("CARGOS_GRUPOS sumiu de cargos.js")
        .to_owned();
    let usados: BTreeSet<&str> = grupos.lines().filter_map(|l| l.split('"').nth(1)).collect();
    assert!(usados.len() >= 4, "grupoDe parou de ser lido: {usados:?}");
    for grupo in usados {
        assert!(
            titulos.contains(&format!("\n    {grupo}: \"")),
            "o grupo «{grupo}» não tem título"
        );
        assert!(
            ordem.contains(&format!("\"{grupo}\"")),
            "o grupo «{grupo}» não tem lugar em CARGOS_GRUPOS"
        );
    }
}

#[test]
fn a_tela_de_cargos_nao_decide_quem_pode() {
    // Quem pode é o servidor (contrato E6): o `cargos.js` desenha o motivo que
    // chega e a regra que chega. Um nome de permissão escrito no script é uma
    // decisão tomada aqui — «se for BANIR, esconde» —, e é exatamente o que
    // não pode haver. Os nomes moram nas frases, para serem DITOS.
    //
    // A exceção é UMA, declarada e explicada no arquivo: `CARGOS_GERIR`, que
    // lê do `SuasPermissoes` se quem pede gere cargos e se tem tudo, para
    // desenhar. Ela sai do texto antes da busca, e só ela.
    let script = sem_comentarios(&read("ui/cargos.js"));
    let excecao = script
        .split("const CARGOS_GERIR = Object.freeze(")
        .nth(1)
        .and_then(|d| d.split(");").next())
        .expect("CARGOS_GERIR sumiu");
    assert!(
        excecao.matches(':').count() == 2,
        "CARGOS_GERIR cresceu: a exceção era duas permissões, e não é lista para crescer: {excecao}"
    );
    let script = script.replace(excecao, "");
    let decididas: Vec<String> = variantes_de_permission()
        .into_iter()
        .filter(|variante| {
            script.contains(&format!("\"{variante}\"")) || script.contains(&format!("'{variante}'"))
        })
        .collect();
    assert!(
        decididas.is_empty(),
        "cargos.js compara nomes de permissão, e a tela passou a decidir quem pode: {decididas:?}"
    );
}

#[test]
fn todo_pedido_de_cargos_passa_pela_sessao_de_origem() {
    // `cargosPedir` aplica a resposta ao estado da sessão que pediu e só deixa
    // desenhar se ela ainda é a da tela (E3). Um `invoke` direto noutro ponto
    // aplicaria a resposta ao que estiver na frente.
    //
    // Os `invoke("…")` moram todos na tabela `CARGOS_COMANDOS`, um literal por
    // comando (é como `frontend.rs` os liga ao `generate_handler!`), e a
    // tabela só é lida por `cargosPedir`.
    let script = sem_comentarios(&read("ui/cargos.js"));
    let tabela = script
        .split("const CARGOS_COMANDOS = Object.freeze({")
        .nth(1)
        .expect("CARGOS_COMANDOS sumiu")
        .split("\n});")
        .next()
        .unwrap_or_default()
        .to_owned();
    let na_tabela = tabela.matches("invoke(\"").count();
    let no_arquivo = script.matches("invoke(").count();
    assert!(na_tabela >= 9, "a tabela de comandos encolheu: {na_tabela}");
    assert_eq!(
        no_arquivo, na_tabela,
        "cargos.js chama `invoke(` fora de CARGOS_COMANDOS ({no_arquivo} no arquivo, {na_tabela} na tabela)"
    );
    let leituras = script.matches("CARGOS_COMANDOS[").count();
    let pedir = script
        .split("async function cargosPedir(")
        .nth(1)
        .expect("cargosPedir sumiu")
        .split("\n}\n")
        .next()
        .unwrap_or_default();
    assert_eq!(leituras, 1, "CARGOS_COMANDOS é lido fora de cargosPedir");
    assert!(
        pedir.contains("CARGOS_COMANDOS["),
        "cargosPedir não usa a tabela de comandos"
    );
    assert!(
        pedir.contains("ForaDaVista"),
        "cargosPedir deixou de reconhecer a resposta de outra sessão"
    );
}

#[test]
fn o_estado_de_cargos_mora_na_sessao_e_nao_no_topo_do_arquivo() {
    // O quadro, o rascunho, a pessoa escolhida e a atribuição pendente são
    // de uma sessão (E3b). Um `let` novo no topo do arquivo é estado que
    // atravessa a troca de vista: o rascunho de A aparecendo em B.
    let script = sem_comentarios(&read("ui/cargos.js"));
    let soltos: BTreeSet<String> = script
        .lines()
        .filter_map(|linha| linha.strip_prefix("let "))
        .filter_map(|resto| resto.split([' ', '=', ';']).next())
        .map(str::to_owned)
        .collect();
    let esperados: BTreeSet<String> = [
        "cargosNaTela",
        "cargosFocoDeVolta",
        "cargosVigia",
        "cargosEsperaDaPrevia",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(
        soltos, esperados,
        "o estado de topo de cargos.js mudou; o que é de uma sessão vai em `cargosPorSessao`"
    );
    assert!(
        script.contains("const cargosPorSessao = new Map();"),
        "o estado por sessão sumiu"
    );
}

#[test]
fn a_chave_de_permissao_de_mod_vai_estruturada_ao_servidor() {
    // `cargos-decisoes.md` §1b: a chave de MOD é `{ mod_id, acao }`, nunca um
    // texto livre. O que a tela guarda como texto volta estruturado no fio.
    let script = sem_comentarios(&read("ui/cargos.js"));
    let para_o_fio = script
        .split("function cargosChaveParaOFio(")
        .nth(1)
        .expect("cargosChaveParaOFio sumiu")
        .split("\n}\n")
        .next()
        .unwrap_or_default();
    assert!(
        para_o_fio.contains("DeMod") && para_o_fio.contains("Produto"),
        "a chave deixou de ir como `Permissao` (Produto / DeMod): {para_o_fio}"
    );
    let partes = script
        .split("function cargosPartesDeMod(")
        .nth(1)
        .expect("cargosPartesDeMod sumiu")
        .split("\n}\n")
        .next()
        .unwrap_or_default();
    assert!(
        partes.contains("mod_id") && partes.contains("acao"),
        "a ChaveDeMod perdeu um campo: {partes}"
    );
    let salvar = script
        .split("async function cargosSalvar(")
        .nth(1)
        .expect("cargosSalvar sumiu")
        .split("\nasync function ")
        .next()
        .unwrap_or_default();
    assert!(
        salvar.contains("cargosListasParaOFio("),
        "SALVAR deixou de mandar as chaves pelo conversor do fio"
    );
    assert!(
        salvar.contains("rascunho.base.revisao"),
        "SALVAR deixou de mandar a revisão do cargo em que a edição começou"
    );
}

#[test]
fn a_folha_de_cargos_usa_so_tokens_e_nada_do_vermelho_de_alerta() {
    // O vermelho é exclusivo de erro e de queda (`specs/07`). Um controle
    // desligado, uma restrição ou um cargo acima do seu não são erro: o acento
    // é o laranja, e a recusa do servidor usa o `.erro` de `base.css`.
    let folha = sem_comentarios(&read("ui/cargos.css"));
    assert!(!folha.contains('#'), "cargos.css tem cor literal");
    for proibido in [
        "--seele-vermelho-alerta",
        "--seele-vermelho-fraco",
        "rgb(",
        "hsl(",
    ] {
        assert!(!folha.contains(proibido), "cargos.css usa {proibido}");
    }
    // Toda classe da folha é da camada: `cargos-…`, o seletor do véu, ou uma
    // classe de folha COMPARTILHADA sendo refinada (`.cargos-ordem-botao .glifo`),
    // que é a exceção que `no_two_screens_claim_the_same_class_name` também faz.
    let compartilhadas: BTreeSet<String> = ["ui/base.css", "ui/acessibilidade.css"]
        .iter()
        .flat_map(|folha| {
            sem_comentarios(&read(folha))
                .split('.')
                .skip(1)
                .map(|pedaco| {
                    pedaco
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                        .collect::<String>()
                })
                .filter(|nome| !nome.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();
    let mut alheias = BTreeSet::new();
    for pedaco in folha.split('.').skip(1) {
        let nome: String = pedaco
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let numero = nome.chars().next().is_some_and(|c| c.is_ascii_digit());
        if !nome.is_empty()
            && !numero
            && !nome.starts_with("cargos-")
            && nome != "camada-cargos"
            && !compartilhadas.contains(&nome)
        {
            alheias.insert(nome);
        }
    }
    assert!(
        alheias.is_empty(),
        "cargos.css estiliza classes de outras folhas: {alheias:?}"
    );
}

#[test]
fn a_camada_de_cargos_e_um_dialogo_com_abas_de_verdade() {
    let pagina = read("ui/index.html");
    let Some(secao) = pagina.split("<!-- cargos (cargos-ui) -->\n<!--").nth(1) else {
        panic!("a seção marcada de cargos sumiu do index.html");
    };
    let secao = secao
        .split("<!-- /cargos (cargos-ui) -->")
        .next()
        .unwrap_or_default();
    let abertura = secao
        .split("<div id=\"cargos\"")
        .nth(1)
        .expect("#cargos sumiu");
    let abertura = abertura.split('>').next().unwrap_or_default();
    for atributo in [
        "role=\"dialog\"",
        "aria-modal=\"true\"",
        "aria-labelledby=\"cargos-titulo\"",
        "hidden",
    ] {
        assert!(
            abertura.contains(atributo),
            "#cargos perdeu {atributo}: <div id=\"cargos\"{abertura}>"
        );
    }
    assert!(
        secao.contains("role=\"tablist\""),
        "as abas perderam o tablist"
    );
    for aba in ["cargos", "pessoas", "explicar"] {
        let botao = secao
            .split(&format!("id=\"cargos-aba-{aba}\""))
            .nth(1)
            .and_then(|d| d.split('>').next())
            .unwrap_or_else(|| panic!("a aba {aba} sumiu"));
        assert!(botao.contains("role=\"tab\""), "a aba {aba} não é role=tab");
        assert!(
            botao.contains(&format!("aria-controls=\"cargos-painel-{aba}\"")),
            "a aba {aba} não aponta o painel dela"
        );
        let painel = secao
            .split(&format!("id=\"cargos-painel-{aba}\""))
            .nth(1)
            .and_then(|d| d.split('>').next())
            .unwrap_or_else(|| panic!("o painel {aba} sumiu"));
        assert!(
            painel.contains("role=\"tabpanel\""),
            "o painel {aba} não é tabpanel"
        );
    }
    // Os alertas de recusa nascem escondidos: `role="alert"` lê o que é
    // escrito depois de revelado (`cargosMostrarErro`).
    for alerta in ["cargos-erro", "cargos-form-erro", "cargos-pessoa-erro"] {
        let tag = secao
            .split(&format!("id=\"{alerta}\""))
            .nth(1)
            .and_then(|d| d.split('>').next())
            .unwrap_or_else(|| panic!("#{alerta} sumiu"));
        assert!(
            tag.contains("role=\"alert\"") && tag.contains("hidden"),
            "#{alerta}: {tag}"
        );
    }
    let script = read("ui/cargos.js");
    assert!(
        script.contains("fecharAoClicarFora(\"cargos\""),
        "a camada não fecha apertando fora"
    );
}

/// As variantes de um enum de `seele-proto/src/cargos.rs`, com os campos
/// delas (vazio para a variante sem campo).
fn variantes_de(tipo: &str) -> Vec<(String, Vec<String>)> {
    let fonte = read("../../crates/seele-proto/src/cargos.rs");
    let Some(corpo) = fonte.split(&format!("pub enum {tipo} {{")).nth(1) else {
        panic!("`pub enum {tipo}` sumiu de seele-proto/src/cargos.rs, e este guarda ficou cego");
    };
    let corpo = sem_comentarios(corpo.split("\n}").next().unwrap_or_default());
    let mut variantes: Vec<(String, Vec<String>)> = Vec::new();
    let mut dentro = false;
    for linha in corpo
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        if dentro {
            if linha.starts_with('}') {
                dentro = false;
            } else if let Some((campo, _)) = linha.split_once(':') {
                if let Some((_, campos)) = variantes.last_mut() {
                    campos.push(campo.trim().to_owned());
                }
            }
            continue;
        }
        let nome: String = linha
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if nome.is_empty() {
            continue;
        }
        variantes.push((nome, Vec::new()));
        dentro = linha.ends_with('{');
    }
    assert!(
        variantes.len() >= 5,
        "só {} variantes de {tipo} foram lidas",
        variantes.len()
    );
    variantes
}

#[test]
fn toda_recusa_de_cargo_do_servidor_tem_a_frase_dela() {
    // A recusa do servidor aparece com a frase do motivo dela (cargos-api §6:
    // uma por variante de `RecusaDeCargo`). Uma variante nova sem frase
    // chegaria à tela como «algo falhou», que é o produto sabendo e não
    // contando. E a variante com campo (a pessoa, a permissão) tem frase que
    // os USA: «isto daria a Bia BANIR», e não «daria a alguém algo».
    let recusas = entrada_das_frases("recusa");
    let mut faltam = Vec::new();
    for (variante, campos) in variantes_de("RecusaDeCargo") {
        let Some(entrada) = recusas.split(&format!("\n    {variante}: ")).nth(1) else {
            faltam.push(variante);
            continue;
        };
        // Até a próxima entrada: uma linha com exatamente quatro espaços e um
        // nome. As continuações de uma frase têm seis, e são dela.
        let entrada: String = entrada
            .lines()
            .enumerate()
            .take_while(|(i, linha)| {
                *i == 0
                    || !(linha.starts_with("    ")
                        && linha
                            .chars()
                            .nth(4)
                            .is_some_and(|c| c.is_ascii_alphabetic()))
            })
            .map(|(_, linha)| linha)
            .collect::<Vec<_>>()
            .join("\n");
        for campo in campos
            .iter()
            .filter(|c| ["pessoa", "permissao"].contains(&c.as_str()))
        {
            assert!(
                entrada.contains(&format!("${{{campo}}}")),
                "a frase de {variante} não diz a {campo} que o servidor mandou: {entrada}"
            );
        }
    }
    assert!(faltam.is_empty(), "recusas de cargo sem frase: {faltam:?}");
}

#[test]
fn todo_motivo_da_explicacao_tem_regra_na_tela() {
    // «Por que ela pode ou não pode» vem do `Motivo` do servidor; a tela traduz
    // cada variante numa regra com frase. Uma variante sem tradução cairia em
    // «nenhum cargo concede», que pode ser mentira.
    let script = sem_comentarios(&read("ui/cargos.js"));
    let traducao = script
        .split("function cargosRegraDoMotivo(")
        .nth(1)
        .expect("cargosRegraDoMotivo sumiu")
        .split("\n}\n")
        .next()
        .unwrap_or_default()
        .to_owned();
    let mut faltam = Vec::new();
    for (variante, _) in variantes_de("Motivo") {
        // `NinguemConcede` é o caso que sobra de propósito.
        if variante != "NinguemConcede" && !traducao.contains(&format!("{variante}:")) {
            faltam.push(variante);
        }
    }
    assert!(
        faltam.is_empty(),
        "variantes de Motivo sem regra na tela: {faltam:?}"
    );
}
