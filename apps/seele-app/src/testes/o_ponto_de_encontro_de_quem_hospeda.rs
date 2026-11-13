/// As duas formas, partidas para não casarem com este texto.
const COM_O_PADRAO: &str = concat!("Hospedagem::", "iniciar_com_o_ponto_padrao(");
const SEM_O_PADRAO: &str = concat!("Hospedagem::", "iniciar(");

/// Este arquivo sem comentários e sem os módulos `#[cfg(test)]`. Cada um é
/// de primeiro nível e fecha numa linha `}` sozinha, onde a produção
/// recomeça. Só os módulos: um `use` sob `#[cfg(test)]` não tem `}`, e
/// cortar até a próxima levaria junto a produção que vem depois dele.
fn producao() -> String {
    producao_de(&super::fonte_sem_comentarios())
}

/// [`producao`] sobre um texto qualquer, para o recorte ter prova própria.
///
/// **E ele se confere sozinho** (sugestão da F5, ao aceitar o recorte): o
/// que se corta é um módulo, e dentro de um módulo toda linha é recuada,
/// menos a do nome e a do `}` final. Um `#[cfg(test)] mod x;`, **sem
/// corpo** (o módulo mora noutro arquivo), não tem o que cortar: só a linha
/// dele sai, e o resto volta à produção. Sem isso o corte iria até a `}` da
/// próxima função de produção — e a conferência da coluna zero acusa
/// qualquer corte que ainda leve produção junto.
fn producao_de(fonte: &str) -> String {
    let mut pedacos = fonte.split(concat!("\n#[cfg(test)]\n", "mod "));
    let mut producao = pedacos.next().unwrap_or_default().to_owned();
    for pedaco in pedacos {
        let (nome, resto) = pedaco.split_once('\n').unwrap_or((pedaco, ""));
        if nome.trim_end().ends_with(';') {
            producao.push('\n');
            producao.push_str(resto);
            continue;
        }
        if let Some(fim) = pedaco.find("\n}\n") {
            let soltas: Vec<&str> = pedaco[..fim]
                .lines()
                .skip(1)
                .filter(|linha| linha.starts_with(|c: char| !c.is_whitespace()))
                .collect();
            assert!(
                soltas.is_empty(),
                "o recorte de um módulo de teste levou linhas da coluna zero, que são de \
                     produção: {soltas:?}"
            );
            producao.push_str(&pedaco[fim + 2..]);
        }
    }
    producao
}

/// **Um módulo de teste sem corpo não leva a produção junto**: o
/// `#[cfg(test)] mod fora;` sai, e a função de produção logo depois dele
/// continua no recorte. E um módulo de teste com corpo sai inteiro.
#[test]
fn o_recorte_tira_o_modulo_de_teste_sem_corpo_e_deixa_a_producao() {
    let fonte = concat!(
        "fn antes() {}\n",
        "#[cfg(test)]\n",
        "mod fora;\n",
        "\n",
        "async fn hospedar_de_producao() {\n",
        "    let _ = 1;\n",
        "}\n",
        "#[cfg(test)]\n",
        "mod dentro {\n",
        "    fn so_de_teste() {}\n",
        "}\n",
        "fn depois() {}\n",
    );
    let recortada = producao_de(fonte);
    assert!(
        recortada.contains("async fn hospedar_de_producao() {"),
        "o recorte levou a produção que vem depois de um módulo de teste sem corpo: {recortada}"
    );
    assert!(recortada.contains("fn antes() {}") && recortada.contains("fn depois() {}"));
    assert!(
        !recortada.contains("mod fora;") && !recortada.contains("so_de_teste"),
        "o recorte deixou módulos de teste na produção: {recortada}"
    );
}

#[test]
fn nenhuma_hospedagem_deste_arquivo_sobe_sem_o_ponto_padrao() {
    let fonte = producao();
    assert!(
        fonte.contains("async fn hospedar(") && !fonte.contains("mod o_ponto_de_encontro"),
        "o recorte da produção deste arquivo quebrou"
    );
    let sem: Vec<&str> = fonte
        .lines()
        .filter(|linha| linha.contains(SEM_O_PADRAO))
        .map(str::trim)
        .collect();
    assert!(
        sem.is_empty(),
        "este arquivo sobe hospedagem com `{SEM_O_PADRAO}`, que sem `$SEELE_ENCONTRO` não \
             convoca ponto de encontro nenhum: quem hospeda por esse caminho perde o degrau 4 em \
             silêncio. Use `{COM_O_PADRAO}`. Linhas: {sem:?}"
    );
    assert!(
        fonte.contains(COM_O_PADRAO),
        "nenhuma hospedagem deste arquivo usa `{COM_O_PADRAO}`: o app hospeda sem o ponto de \
             encontro do projeto"
    );
}

/// **Quem hospeda com um MOD de API antiga sabe antes de o servidor subir**
/// (tarefa 13 do plano da F3, Step 5b). A conferência é a de
/// `mods::mods_de_api_antiga` (provada lá, com a pasta da 0.15.1); aqui, que
/// ela é chamada com o banco e as raízes que vão ao servidor, e **antes**
/// de marcar o servidor hospedado e de `iniciar`: depois, a recusa chegaria
/// com o servidor já no ar. Tirada a chamada, ou posta depois do `iniciar`,
/// reprova.
#[test]
fn hospedar_confere_os_mods_de_api_antiga_antes_de_subir() {
    let fonte = super::fonte_sem_comentarios();
    let corpo = super::corpo_de(&fonte, concat!("async fn hospedar_em", "("));
    // A conferência é `mods::recusas_de_hospedar` desde o P1 da 0.17: a de
    // `mods_de_api_antiga`, com o mesmo banco e as mesmas raízes, menos os
    // MODs que já têm uma escolha estagiada (aplicada antes da subida).
    let conferir = corpo
        .find("mods::recusas_de_hospedar(")
        .expect("`hospedar` não confere os MODs de API antiga: o servidor subiria calado");
    let marcar = corpo
        .find("session.servidor_hospedado.lock()")
        .expect("`hospedar` deixou de marcar o servidor hospedado");
    let subir = corpo
        .find("subir_o_guardado(")
        .expect("`hospedar` deixou de subir o servidor");
    assert!(
        conferir < marcar && conferir < subir,
        "a conferência dos MODs de API antiga vem depois de marcar o servidor ou de subi-lo: \
             a recusa chegaria com o servidor no ar"
    );
    assert!(
        corpo[conferir..].starts_with("mods::recusas_de_hospedar(&config, id_do_servidor, &banco)"),
        "a conferência não usa o banco e o servidor que vão à subida"
    );
    let mods = include_str!("../mods.rs");
    let regra = super::corpo_de(mods, "pub(crate) fn recusas_de_hospedar(");
    assert!(
        regra.contains(
            "mods_de_api_antiga(banco, &crate::servidores::raizes_dos_mods(config, servidor))"
        ),
        "a regra da recusa não usa as raízes do servidor que sobe"
    );
}

/// **As saídas de quem hospeda leem da janela se há servidor no ar**
/// (tarefa 13 do plano da F3, Step 5b). A recusa é de
/// `mods::resolver_mod_de_api_antiga` (provada lá); aqui, que o comando
/// lhe passa o `guardado` desta janela, o campo do `apagar_servidor`, e
/// não uma constante. Com `false` no lugar, reprova.
#[test]
fn as_saidas_de_quem_hospeda_leem_da_janela_se_ha_servidor_no_ar() {
    let fonte = super::fonte_sem_comentarios();
    let corpo = super::corpo_de(&fonte, "async fn resolver_mod_de_api_antiga(");
    let no_ar = corpo
        .find("guardado.is_some()")
        .expect("as saídas não leem o servidor guardado desta janela: mexeriam num banco de pé");
    let chamada = corpo
        .find("mods::resolver_mod_de_api_antiga(")
        .expect("o comando não chama a regra das saídas");
    assert!(
        no_ar < chamada && corpo[chamada..].contains("no_ar)"),
        "o comando não passa à regra o «no ar» que leu da janela"
    );
}

#[test]
fn hospedar_sobe_com_o_ponto_de_encontro_padrao() {
    let fonte = super::fonte_sem_comentarios();
    let corpo = super::corpo_de(&fonte, concat!("async fn hospedar_em", "("));
    // A subida mora em `subir_o_guardado` desde o modo local (E9): o
    // `hospedar` a chama, e ela sobe com o ponto padrão fora do modo local.
    assert!(
        corpo.contains("subir_o_guardado(") && !corpo.contains(SEM_O_PADRAO),
        "o `hospedar` não sobe pelo `subir_o_guardado`"
    );
    let corpo = super::corpo_de(&fonte, concat!("async fn subir_o_guardado", "("));
    assert!(
        corpo.contains(COM_O_PADRAO) && !corpo.contains(SEM_O_PADRAO),
        "o `hospedar` não sobe o servidor com `{COM_O_PADRAO}`: sem ele, e sem \
             `$SEELE_ENCONTRO`, a hospedagem não convoca ponto de encontro nenhum, e quem hospeda \
             atrás de CGNAT perde o degrau 4 em silêncio"
    );
}
