use super::{
    e_de_um_dominio_de_gif, endereco_de_previa, endereco_do_teste_de_rede, endereco_que_pode_sair,
    midia_declarada_pela_pagina, FalhaNaPrevia, DOMINIOS_DE_GIF,
};

/// **O teste de rede abre na origem do produto, e leva só o nome da falha.**
///
/// O endereço passa pelo mesmo filtro de todo link que sai desta janela, e
/// o que não é um identificador (um endereço, um apelido, um convite) não
/// entra na consulta: ela chega ao registro de acesso de quem serve a
/// página.
#[test]
fn o_teste_de_rede_abre_na_origem_do_produto_e_leva_so_o_nome_da_falha() {
    let origem = seele_ffi::ORIGEM_DO_NAVEGADOR;
    let pagina = format!("https://{origem}/diagnostico-rede.html");
    assert_eq!(endereco_do_teste_de_rede(None), pagina);
    assert_eq!(
        endereco_do_teste_de_rede(Some("NatNaoAbriu")),
        format!("{pagina}?erro=NatNaoAbriu")
    );
    for fora in [
        "",
        "   ",
        "203.0.113.7:8383",
        "casa.exemplo.br",
        "ana maria",
        "seele://casa.exemplo:8383/?fp=ab",
        "a&convite=7K4M",
        "NatNaoAbriu#x",
        "UmNomeDeFalhaQueNaoExisteEPassaDoTetoDeQuarentaLetras",
    ] {
        assert_eq!(
            endereco_do_teste_de_rede(Some(fora)),
            pagina,
            "`{fora}` entrou no endereço do teste de rede"
        );
    }
    for endereco in [
        endereco_do_teste_de_rede(None),
        endereco_do_teste_de_rede(Some("SemResposta")),
    ] {
        assert_eq!(
            endereco_que_pode_sair(&endereco),
            Ok(endereco.as_str()),
            "o filtro de saída recusou o endereço do teste de rede"
        );
    }
}

/// **A lista de domínios é fechada, e o sufixo é conferido com o ponto.**
///
/// O defeito que este teste existe para impedir é de uma linha: um
/// `ends_with("tenor.com")` sem ponto deixa `tenor.com.exemplo.br` passar,
/// e aí quem cola um link escolhe o domínio que esta janela busca — que é
/// exatamente o que a lista fechada existe para não permitir.
#[test]
fn so_os_dominios_da_lista_tem_a_pagina_resolvida() {
    for bom in [
        "https://tenor.com/view/gato-dancando-gif-12345",
        "https://www.tenor.com/view/x-gif-1",
        "https://media.tenor.com/abc/dancando.gif",
        "https://GIPHY.com/gifs/algo",
        "https://i.imgur.com/abc.gif",
    ] {
        assert!(e_de_um_dominio_de_gif(bom), "recusou `{bom}`");
    }
    for hostil in [
        // O sufixo sem o ponto: o caso que a comparação ingênua deixa passar.
        "https://tenor.com.exemplo.br/colher-ip",
        "https://naotenor.com/x",
        "https://exemplo.br/tenor.com/x",
        "https://exemplo.br/?a=tenor.com",
        "https://192.168.0.1/roteador",
        "nao e um endereco",
    ] {
        assert!(!e_de_um_dominio_de_gif(hostil), "aceitou `{hostil}`");
    }
}

/// **O que a página declara é lido de `og:image`, em qualquer ordem.**
///
/// A ordem dos atributos é de quem escreve o HTML, e um leitor que exigisse
/// `property` antes de `content` funcionaria num dos três sites e não nos
/// outros — a forma de defeito que só aparece em produção, num site só.
#[test]
fn a_midia_declarada_e_lida_venha_em_que_ordem_vier() {
    let casos = [
        r#"<meta property="og:image" content="https://media.tenor.com/a.gif">"#,
        r#"<meta content="https://media.tenor.com/a.gif" property="og:image"/>"#,
        r#"<meta   property = 'og:image'   content = 'https://media.tenor.com/a.gif' >"#,
        r#"<META PROPERTY="OG:IMAGE" CONTENT="https://media.tenor.com/a.gif">"#,
        r#"<meta name="twitter:image" content="https://media.tenor.com/a.gif">"#,
        r#"<meta property="og:image:secure_url" content="https://media.tenor.com/a.gif">"#,
    ];
    for html in casos {
        assert_eq!(
            midia_declarada_pela_pagina(html).as_deref(),
            Some("https://media.tenor.com/a.gif"),
            "não leu: {html}"
        );
    }
}

/// `&amp;` volta a ser `&`: um endereço de mídia carrega consulta.
#[test]
fn a_entidade_do_e_comercial_volta_ao_endereco() {
    let html = r#"<meta property="og:image" content="https://media.tenor.com/a.gif?w=1&amp;h=2">"#;
    assert_eq!(
        midia_declarada_pela_pagina(html).as_deref(),
        Some("https://media.tenor.com/a.gif?w=1&h=2")
    );
}

/// **Uma página sem declaração não vira imagem nenhuma.**
///
/// E o que ela traz de parecido não conta: um `<img>` no corpo é desenho da
/// página, não o que ela publicou para quem compartilha o link.
#[test]
fn uma_pagina_sem_declaracao_nao_da_midia() {
    for html in [
        "<html><body><img src=\"https://media.tenor.com/a.gif\"></body></html>",
        r#"<meta name="description" content="um gato dançando">"#,
        r#"<meta property="og:title" content="og:image">"#,
        r#"<meta property="og:image" content="">"#,
        r#"<meta property="og:image">"#,
        "",
    ] {
        assert_eq!(midia_declarada_pela_pagina(html), None, "leu de: {html}");
    }
}

/// A lista existe e é pequena: ela é uma troca de privacidade, e uma troca
/// que cresce sem ninguém olhar deixa de ser uma troca.
#[test]
fn a_lista_de_dominios_e_curta_e_nomeada() {
    assert!(
        DOMINIOS_DE_GIF.len() <= 5,
        "a lista cresceu sem quem a defendesse: {DOMINIOS_DE_GIF:?}"
    );
    for dominio in DOMINIOS_DE_GIF {
        assert!(
            dominio.contains('.') && !dominio.starts_with('.'),
            "`{dominio}` não é um domínio registrável"
        );
    }
}

/// **A busca de uma imagem de link tem três travas, e elas têm de estar lá.**
///
/// Esta busca é a única do produto que vai a um endereço que outra pessoa
/// escolheu. O que a segura:
///
///   - **o esquema**, pela mesma lista do `abrir_no_navegador`. Sem ela,
///     um `file://` colado na conversa faria esta janela ler o disco;
///   - **o teto, enquanto chega.** Confiar no `Content-Length` é confiar
///     em quem está do outro lado, e juntar tudo para medir depois é
///     deixá-lo escolher quanta memória esta janela gasta;
///   - **os bytes contra o tipo alegado**, pela mesma regra do anexo. Um
///     endereço que promete `image/gif` e entrega HTML é recusado.
///
/// Guarda de texto-fonte porque as três só se exercitam contra um servidor
/// de verdade, e um teste que levantasse um seria um teste que sai à rede.
///
/// **As travas moram em `buscar_com_teto`**, e não no comando, desde que a
/// prévia passou a fazer duas buscas — a página e a mídia que ela declara.
/// Isso é o ponto: uma segunda busca escrita à mão, ao lado, é uma segunda
/// busca sem as três travas, e este teste é o que obriga as duas a
/// passarem pela mesma porta.
/// **A imagem de um link que declara pixels demais não vai à janela, e a
/// recusa leva as medidas e o teto** (a varredura do incidente da capa em
/// 4K, 11/11/2026). São 69 bytes de PNG que declaram 30 000 × 30 000: cabem
/// no teto de bytes da busca, e abertos pediriam 3,6 GB ao `<img>` da
/// conversa. A que cabe sai como `data:`, com o tipo dos bytes.
#[test]
fn a_imagem_de_link_com_pixels_demais_e_recusada_com_os_numeros() {
    let bomba = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x75\x30\x00\x00\x75\x30\x08\x06\x00\x00\x00\x66\x27\xf8\xba\x00\x00\x00\x0cIDAT\x78\xda\x63\x60\xa0\x0c\x00\x00\x00\x40\x00\x01\x89\xc9\xaf\x43\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    let recusa = super::imagem_de_link_pronta(crate::reducao::previa_na_janela(bomba.to_vec()))
        .expect_err("uma imagem de 900 megapixels foi à janela");
    assert_eq!(
        serde_json::to_value(&recusa).expect("a recusa atravessa a ponte"),
        serde_json::json!({ "PixelsDemais": { "largura": 30000, "altura": 30000, "teto": 36_000_000 } }),
        "a janela lê as medidas e o teto desta forma, para dizê-los no lugar da imagem"
    );
    let pequena = crate::pacote_de_teste::PNG;
    assert_eq!(
        super::imagem_de_link_pronta(crate::reducao::previa_na_janela(pequena.to_vec())).ok(),
        Some(seele_ffi::preview::data_uri(
            seele_ffi::preview::ImageFormat::Png,
            pequena
        )),
    );
}

#[test]
fn a_busca_de_imagem_de_link_confere_esquema_teto_e_bytes() {
    let sem_comentario = super::fonte_sem_comentarios();
    let busca = super::corpo_de(&sem_comentario, "async fn buscar_com_teto(");

    assert!(
        busca.contains("endereco_de_previa(&texto)?"),
        "a busca deixou de conferir o destino: um `file://` colado na \
             conversa faria esta janela ler o disco de quem lê, e um \
             `http://127.0.0.1` a faria bater num serviço da máquina dela: \
             {busca}"
    );
    assert!(
        busca.contains("PREVIEW_LIMIT") && busca.contains(".chunk()"),
        "o teto deixou de ser aplicado enquanto os bytes chegam: quem \
             responde escolhe quanta memória esta janela gasta: {busca}"
    );

    let comando = super::corpo_de(&sem_comentario, "async fn previa_de_link(");
    // **Nenhuma das duas buscas monta o `data:` direto dos bytes que
    // vieram**: as duas passam pelo teto de pixels e pela redução
    // (`imagem_de_link_para_a_janela`). O teto de bytes não limita o que a
    // imagem ocupa aberta, e quem controla o endereço escolhe as medidas.
    assert!(
        comando
            .matches("imagem_de_link_para_a_janela(bytes).await")
            .count()
            == 2
            && !comando.contains("data_uri("),
        "uma das duas buscas voltou a mandar a imagem à janela do tamanho \
             que veio, sem teto de pixels: {comando}"
    );
    let pronta = super::corpo_de(&sem_comentario, "async fn imagem_de_link_para_a_janela(");
    assert!(
        pronta.contains("reducao::previa_na_janela(bytes)"),
        "a imagem de um link deixou de passar pelo teto de pixels da prévia: {pronta}"
    );
    assert!(
        comando.matches("judge(&alegado, &bytes)").count() == 2,
        "uma das duas buscas deixou de conferir os bytes contra o tipo \
             alegado: um endereço que promete imagem e entrega outra coisa \
             passa: {comando}"
    );
    // **A segunda busca é a que a página escolhe**, e é por isso que ela
    // tem uma trava a mais: sem a conferência de domínio, uma página desta
    // lista apontaria esta janela para qualquer endereço do mundo, e a
    // lista fechada não teria fechado nada.
    assert!(
        comando.contains("if !e_de_um_dominio_de_gif(&midia)"),
        "a mídia declarada por uma página deixou de ser conferida contra a \
             lista de domínios: {comando}"
    );
    assert!(
        comando.contains("alegado.starts_with(\"text/html\")"),
        "a resolução de página deixou de exigir que a resposta fosse HTML: \
             {comando}"
    );

    // **Nenhum redirecionamento é seguido por baixo.** Antes do ADR 0053 o
    // cliente os seguia com uma política própria, e essa política conferia
    // o salto pela régua de *abrir um link* — que aceita a rede local. Agora
    // quem os segue é o laço de `buscar_com_teto`, que revalida e resolve
    // cada um; a metade que fica aqui é a proibição de o cliente fazer isso
    // sozinho, porque ele está fixado no endereço do salto anterior.
    let cliente = super::corpo_de(&sem_comentario, "fn cliente_da_previa(");
    assert!(
        cliente.contains("redirect(reqwest::redirect::Policy::none())"),
        "o cliente voltou a seguir redirecionamento por conta própria: um \
             `302` para a rede local atravessa sem passar pela validação do \
             laço, e com o endereço fixado no host errado: {cliente}"
    );
}

/// **O que um link de chat não pode abrir.**
///
/// Cada um destes é um programa que alguém do outro lado da conversa
/// escolheria iniciar na máquina de quem lê. A lista é de casos vistos em
/// produtos que aprenderam isso pelo caminho difícil.
#[test]
fn so_http_e_https_saem_desta_janela() {
    for hostil in [
        "file:///etc/passwd",
        "file://C:/Windows/System32",
        "javascript:alert(1)",
        "data:text/html,<script>x</script>",
        "smb://servidor/compartilhado",
        "vscode://file/etc/hosts",
        "ms-msdt:/id",
        "seele://casa.exemplo",
        "//exemplo.br",
        "exemplo.br",
        "",
        "   ",
        "http://",
        "https://",
    ] {
        assert!(
            endereco_que_pode_sair(hostil).is_err(),
            "deixou sair `{hostil}`"
        );
    }
}

/// E o que **tem** de sair, senão o ajuste não serve para nada.
#[test]
fn um_endereco_comum_atravessa_inteiro() {
    for bom in [
        "https://exemplo.br",
        "http://exemplo.br/pagina?a=1&b=2#parte",
        "HTTPS://EXEMPLO.BR",
        "https://exemplo.br/caminho%20escapado",
    ] {
        assert_eq!(endereco_que_pode_sair(bom), Ok(bom), "recusou `{bom}`");
    }
    // O espaço em volta é de quem colou, e não do endereço.
    assert_eq!(
        endereco_que_pode_sair("  https://exemplo.br  "),
        Ok("https://exemplo.br")
    );
}

/// **O que separa um argumento de dois.** Um endereço com nova linha
/// dentro é a forma clássica de fazer um programa ler duas coisas onde
/// quem conferiu viu uma.
#[test]
fn nada_com_espaco_ou_controle_no_meio_atravessa() {
    for torto in [
        "https://exemplo.br/a b",
        "https://exemplo.br\n/etc/passwd",
        "https://exemplo.br\u{0}",
        "https://exemplo.br\ta",
    ] {
        assert!(
            endereco_que_pode_sair(torto).is_err(),
            "deixou sair `{torto:?}`"
        );
    }
}

/// **Uma prévia não alcança esta máquina, esta rede, nem nada reservado.**
///
/// ADR 0053 e R06. A revisão da v15 verificou que a janela classificava
/// `http://127.0.0.1:9999/private.png` como prévia automática, e que a
/// cadeia nativa faria a busca. Conferir os bytes depois da resposta não
/// evita a requisição — e é a requisição que é o problema.
#[test]
fn uma_previa_nao_alcanca_a_maquina_nem_a_rede_de_quem_le() {
    for privado in [
        // A reprodução literal do review.
        "http://127.0.0.1:9999/private.png",
        "http://localhost/x.png",
        "http://LOCALHOST./x.png",
        "http://algo.local/x.png",
        "http://nas.internal/x.png",
        // O roteador da casa, e as três faixas privadas.
        "http://192.168.0.1/x.png",
        "http://10.0.0.5/x.png",
        "http://172.16.3.9/x.png",
        // `0.0.0.0/8`, que em várias pilhas é «esta máquina».
        "http://0.0.0.0/x.png",
        // Metadados de nuvem, que é link-local.
        "http://169.254.169.254/latest/meta-data",
        // Compartilhado entre operadoras.
        "http://100.100.1.1/x.png",
        // IPv6: loopback, local único, link-local…
        "http://[::1]/x.png",
        "http://[fd00::1]/x.png",
        "http://[fe80::1]/x.png",
        // …e o mapeado, que é o que uma régua só de IPv4 deixa passar.
        "http://[::ffff:127.0.0.1]/x.png",
    ] {
        assert!(
            matches!(
                endereco_de_previa(privado),
                Err(FalhaNaPrevia::DestinoPrivado(_))
            ),
            "a prévia alcançaria `{privado}`"
        );
        // **E a política de abrir um link continua aceitando.** As duas são
        // políticas diferentes de propósito: `http://192.168.0.1` clicado
        // por uma pessoa é o roteador da casa dela, e recusá-lo seria o
        // produto decidindo onde ela pode navegar.
        assert!(
            endereco_que_pode_sair(privado).is_ok(),
            "abrir um link parou de aceitar `{privado}`: as duas políticas \
                 voltaram a ser uma, que é o defeito que o ADR 0053 separou"
        );
    }
}

/// E a internet pública continua passando, senão o guarda acima é uma porta
/// fechada para todo mundo — que passaria no teste de cima sem fazer nada.
#[test]
fn uma_previa_alcanca_a_internet_publica() {
    for publico in [
        "https://exemplo.br/foto.png",
        "https://media.tenor.com/algo.gif",
        "http://8.8.8.8/x.png",
        "http://[2606:4700::1]/x.png",
    ] {
        assert!(
            endereco_de_previa(publico).is_ok(),
            "recusou `{publico}`, que é internet pública"
        );
    }
}

/// **Cada salto é revalidado, e o endereço resolvido é fixado.**
///
/// Guarda de texto-fonte pelo mesmo motivo dos vizinhos: exercitar isto
/// pediria um servidor que redireciona, e um teste que levantasse um seria
/// um teste que sai à rede.
///
/// As três metades que o R06 pede — «restrição de destinos privados/loopback/
/// link-local e nova validação em cada redirecionamento e resolução»:
#[test]
fn cada_salto_da_previa_e_validado_resolvido_e_fixado() {
    let sem_comentario = super::fonte_sem_comentarios();
    let busca = super::corpo_de(&sem_comentario, "async fn buscar_com_teto(");

    assert!(
        busca.contains("endereco_de_previa(&texto)?") && busca.contains("enderecos_publicos_de("),
        "um salto deixou de ser validado e resolvido de novo: um `302` para \
             a rede local atravessa: {busca}"
    );
    assert!(
        busca.contains("reqwest::header::LOCATION"),
        "o laço deixou de seguir os saltos à mão, e quem os seguir por \
             baixo os seguirá sem passar por esta validação: {busca}"
    );

    let cliente = super::corpo_de(&sem_comentario, "fn cliente_da_previa(");
    assert!(
        cliente.contains("resolve_to_addrs(host, enderecos)"),
        "o endereço deixou de ser fixado: o nome é resolvido uma vez para \
             conferir e outra para conectar, e nada obriga as duas respostas a \
             serem a mesma: {cliente}"
    );
    assert!(
        cliente.contains("Policy::none()"),
        "o cliente voltou a seguir redirecionamento por conta própria, com \
             o endereço fixado no host errado: {cliente}"
    );
}

/// **Nenhuma prévia sai antes do consentimento do domínio.**
///
/// A ordem importa: a pergunta vem **antes** de `buscar_com_teto`. Um
/// consentimento conferido depois seria um consentimento pedido depois de a
/// requisição já ter aparecido no servidor de outra pessoa.
#[test]
fn a_previa_pergunta_antes_de_buscar() {
    let sem_comentario = super::fonte_sem_comentarios();
    let comando = super::corpo_de(&sem_comentario, "async fn previa_de_link(");

    let pergunta = comando
        .find("previa_consentida(")
        .expect("o comando deixou de conferir o consentimento");
    let busca = comando
        .find("buscar_com_teto(")
        .expect("o comando deixou de buscar");
    assert!(
        pergunta < busca,
        "o consentimento é conferido depois da busca: a requisição já \
             apareceu no servidor de quem escreveu a mensagem quando alguém \
             for perguntado: {comando}"
    );
}
