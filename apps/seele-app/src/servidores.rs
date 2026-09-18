//! Os servidores que esta máquina guarda, e o banco de cada um.
//!
//! # O que estava errado
//!
//! Hospedar abria sempre o **mesmo** banco: `<config>/seele.db`, um por
//! máquina. Isso já guardava o nome e a imagem entre uma sessão e outra — o que
//! não havia era mais de um. Quem quisesse uma mesa de RPG com um conjunto de
//! MODs e um servidor de conversa sem nenhum tinha de escolher, ou trocar tudo
//! de lugar à mão.
//!
//! O pedido veio nessas palavras: *«exatamente como no Minecraft, onde você
//! pode ter várias instâncias, sendo de versões iguais ou diferentes»*.
//!
//! # A forma, e por que ela não move arquivo nenhum
//!
//! O registro é uma lista em `<config>/servidores.json`, e cada entrada diz
//! **onde** está o banco dela. Um servidor novo mora em
//! `<config>/servidores/<id>/seele.db`; o que já existia continua exatamente
//! onde está.
//!
//! Essa última frase é a decisão inteira. A alternativa — mover `seele.db`
//! para dentro de uma pasta nova na primeira vez que o app subir — parece mais
//! arrumada e arrisca a única coisa que não se pode perder: as conversas de
//! quem já usa. Um `rename` que falha no meio, um disco cheio, um processo
//! morto entre o mover e o gravar, e o servidor da pessoa some. Apontar não
//! tem meio do caminho.
//!
//! Por isso o primeiro servidor de toda máquina que já hospedou é adotado com
//! `caminho: None`, que quer dizer «o de sempre».
//!
//! # O que continua sendo por máquina, e não por servidor
//!
//! Os **arquivos** dos MODs instalados, em `<config>/mods`. Quais deles estão
//! ligados já é por banco — `seele_server::persistence::mods::enabled` lê do
//! banco do servidor —, então dois servidores com o mesmo MOD instalado podem
//! ter um ligado e o outro não. Instalar uma vez e ligar onde interessa é o que
//! o produto já fazia; duplicar os bytes por instância seria trabalho para
//! chegar ao mesmo lugar ocupando mais disco.

use std::path::{Path, PathBuf};

/// Um servidor guardado nesta máquina.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct Servidor {
    /// O identificador no disco. `[a-z0-9-]`, gerado, nunca escrito por quem usa.
    pub(crate) id: String,
    /// O nome que a pessoa deu, ou vazio.
    ///
    /// **Não é a verdade sobre o nome do servidor**, que mora no banco dele e
    /// pode ter sido trocado em CONFIGURAÇÕES desde então. É o que a lista
    /// mostra para se escolher entre um e outro sem abrir os dois.
    #[serde(default)]
    pub(crate) nome: String,
    /// A versão do SEELE em que ele foi criado.
    #[serde(default)]
    pub(crate) versao: String,
    /// `None` é o banco de sempre, `<config>/seele.db`. Ver o cabeçalho.
    #[serde(default)]
    pub(crate) caminho: Option<String>,
    /// Segundos desde a época, para ordenar a lista pelo mais recente.
    #[serde(default)]
    pub(crate) ultimo_uso: i64,
}

/// O arquivo do registro.
fn registro(config: &str) -> PathBuf {
    Path::new(config).join("servidores.json")
}

/// Agora, em segundos desde a época. Zero quando o relógio está antes dela.
fn agora() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Lê o registro, já adotando o servidor antigo quando há um.
///
/// **Ler é o que adota.** Não há passo de migração em outro lugar para alguém
/// esquecer de chamar: a primeira listagem numa máquina que já hospedou
/// encontra `seele.db` e o põe na lista, com `caminho: None`.
pub(crate) fn listar(config: &str) -> Vec<Servidor> {
    let mut lista: Vec<Servidor> = std::fs::read_to_string(registro(config))
        .ok()
        .and_then(|texto| serde_json::from_str(&texto).ok())
        .unwrap_or_default();

    let tem_antigo = Path::new(config).join("seele.db").exists();
    let ja_adotado = lista.iter().any(|s| s.caminho.is_none());
    if tem_antigo && !ja_adotado {
        lista.insert(
            0,
            Servidor {
                id: "principal".to_owned(),
                nome: String::new(),
                versao: String::new(),
                caminho: None,
                ultimo_uso: agora(),
            },
        );
        gravar(config, &lista);
    }

    lista.sort_by_key(|s| std::cmp::Reverse(s.ultimo_uso));
    lista
}

/// Grava o registro. Uma falha aqui não derruba o que está no ar.
fn gravar(config: &str, lista: &[Servidor]) {
    let Ok(texto) = serde_json::to_string_pretty(lista) else {
        return;
    };
    let _ = std::fs::create_dir_all(config);
    if let Err(erro) = std::fs::write(registro(config), texto) {
        tracing::warn!(%erro, "não deu para gravar o registro de servidores");
    }
}

/// Um identificador livre, a partir do que a pessoa escreveu.
///
/// O nome vira a raiz do identificador para que a pasta no disco diga alguma
/// coisa a quem for olhar. Um nome só de acentos e espaços vira `servidor`, e
/// o número no fim é o que garante que dois nomes iguais não briguem.
fn identificador(nome: &str, usados: &[Servidor]) -> String {
    let raiz: String = nome
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let raiz = if raiz.is_empty() {
        "servidor".to_owned()
    } else {
        raiz.chars().take(32).collect()
    };

    if !usados.iter().any(|s| s.id == raiz) {
        return raiz;
    }
    for n in 2..1000 {
        let tentativa = format!("{raiz}-{n}");
        if !usados.iter().any(|s| s.id == tentativa) {
            return tentativa;
        }
    }
    format!("{raiz}-{}", agora())
}

/// Põe um servidor novo no registro e devolve o que foi guardado.
pub(crate) fn criar(config: &str, nome: &str, versao: &str) -> Servidor {
    let mut lista = listar(config);
    let id = identificador(nome, &lista);
    let novo = Servidor {
        caminho: Some(format!("servidores/{id}/seele.db")),
        id,
        nome: nome.trim().to_owned(),
        versao: versao.to_owned(),
        ultimo_uso: agora(),
    };
    lista.push(novo.clone());
    gravar(config, &lista);
    novo
}

/// O banco daquele servidor, com a pasta dele já criada.
///
/// Um id que não está no registro devolve o banco de sempre. É o desfecho
/// certo: sem registro nenhum — máquina nova, ou arquivo apagado — hospedar
/// continua funcionando como sempre funcionou, e não com um erro sobre um
/// arquivo de registro que quem usa nunca soube que existia.
pub(crate) fn banco(config: &str, id: Option<&str>) -> PathBuf {
    let padrao = Path::new(config).join("seele.db");
    let Some(id) = id else { return padrao };
    let Some(servidor) = listar(config).into_iter().find(|s| s.id == id) else {
        return padrao;
    };
    let Some(relativo) = servidor.caminho else {
        return padrao;
    };
    let caminho = Path::new(config).join(relativo);
    if let Some(pasta) = caminho.parent() {
        let _ = std::fs::create_dir_all(pasta);
    }
    uma_chave_por_maquina(config, &caminho);
    caminho
}

/// Faz este banco apresentar a **mesma** chave que o resto da máquina.
///
/// **Relatado do uso real, e o defeito era meu:** «deu erro quando saí de um
/// server e tentei entrar em outro — A CHAVE DO SERVIDOR MUDOU».
///
/// A identidade TLS mora no banco, e cada servidor guardado ganhou banco
/// próprio. A chave do pino do TOFU, porém, é **o texto do alvo**: o mesmo
/// endereço para todos eles. Trocar de servidor passou a disparar o alerta
/// bloqueante do ADR 0003, o que é reservado para ataque.
///
/// O `tls.rs` já carregava a lição, escrita quando reiniciar o `seeled` trocava
/// a chave: «um reinício de rotina disparando o aviso reservado para ataque é
/// pior que não ter o aviso: ensina a ignorá-lo». Repeti a mesma numa forma
/// nova.
///
/// **Por que aqui dentro, e não num passo ao lado.** A primeira versão era uma
/// função que `hospedar` chamava, e a prova por reversão a desmascarou: apagar
/// a chamada não fazia teste nenhum ficar vermelho, porque o teste chamava a
/// função direto. Um passo que se pode esquecer é um passo que se esquece. Aqui
/// não há o que esquecer: quem pede o caminho do banco recebe a chave plantada
/// junto, e não há como pedir um sem o outro.
///
/// A chave herdada é a do **banco de sempre**, e não a do primeiro que subir. É
/// o que preserva o pino que as pessoas já têm.
///
/// Silenciosa por escolha: não herdar significa que o servidor gera a própria
/// chave, que é o comportamento de antes desta função existir.
fn uma_chave_por_maquina(config: &str, banco: &Path) {
    use seele_server::persistence::{Location, Persistence};

    let legado = Path::new(config).join("seele.db");
    if banco == legado || !legado.exists() {
        return;
    }
    let (Ok(de), Ok(para)) = (
        Persistence::open(&Location::File(legado)),
        Persistence::open(&Location::File(banco.to_path_buf())),
    ) else {
        return;
    };
    match seele_server::tls::herdar_identidade(&de, &para) {
        Ok(true) => tracing::info!("o servidor novo herdou a chave desta máquina"),
        Ok(false) => {}
        Err(erro) => tracing::warn!(%erro, "não deu para herdar a chave desta máquina"),
    }
}

/// Anota que este servidor foi usado agora, para a lista vir na ordem certa.
pub(crate) fn marcar_uso(config: &str, id: &str) {
    let mut lista = listar(config);
    let Some(servidor) = lista.iter_mut().find(|s| s.id == id) else {
        return;
    };
    servidor.ultimo_uso = agora();
    gravar(config, &lista);
}

/// Guarda o nome numa entrada que já existe, para a lista não mentir.
///
/// Chamado quando quem hospeda renomeia o servidor: sem isto, a lista
/// continuaria mostrando o nome do dia da criação, e escolher entre dois
/// servidores pelo nome errado é pior do que escolher por um id cru.
pub(crate) fn renomear(config: &str, id: &str, nome: &str) {
    let mut lista = listar(config);
    let Some(servidor) = lista.iter_mut().find(|s| s.id == id) else {
        return;
    };
    servidor.nome = nome.trim().to_owned();
    gravar(config, &lista);
}

/// Onde moram o pacote e os arquivos mutáveis dos MODs **desta instância**.
///
/// Plano de isolamento de 18/09, P0 «arquivos mutáveis de servidores
/// diferentes compartilham caminho». Antes havia uma raiz só, e o `dados/`
/// saía de dentro do pacote: `config/mods/<autor>/<nome>/dados`. Toda
/// instância hospedada nesta máquina recebia a mesma, então o mesmo MOD em
/// dois servidores escrevia nos mesmos arquivos — um avatar gravado num
/// aparecia no outro.
///
/// O pacote continua compartilhado, e isso é o certo: ele é imutável e
/// conferido por hash, e dois servidores que exigem os mesmos bytes exigem os
/// mesmos bytes.
///
/// # O legado, que tem dados de verdade lá dentro
///
/// A instância adotada — `caminho: None`, que é toda máquina que hospedou
/// antes de haver lista — tem perfis, avatares e estado de campanha em
/// `mods/<id>/dados`. Mudar a raiz dela sem mais nada seria abrir o servidor
/// com tudo em branco, que quem usa leria como perda.
///
/// Então a pasta é **movida**, uma vez, na primeira vez que a raiz é pedida. A
/// mudança é um `rename` dentro do mesmo volume: ou ela acontece inteira, ou
/// não acontece. Se não acontecer, a raiz antiga continua valendo — os dados
/// nunca ficam a meio caminho, e um servidor único não sofre do defeito que
/// esta separação conserta.
pub(crate) fn raizes_dos_mods(config: &str, id: Option<&str>) -> seele_server::RaizesDosMods {
    let base = Path::new(config);
    let pacotes = base.join("mods");
    let dados = match id {
        Some(id) => base.join("servidores").join(id).join("mod-data"),
        None => base.join("mod-data-legado"),
    };
    if id.is_none() {
        mudar_os_dados_do_legado(&pacotes, &dados);
    }
    seele_server::RaizesDosMods { pacotes, dados }
}

/// Move `mods/<id>/dados` para a raiz nova, uma vez, por MOD.
///
/// Silencioso quando não há o que mover — que é o caso de toda máquina depois
/// da primeira vez. Falhar não interrompe: quem chama recebe a raiz nova de
/// qualquer forma, e um MOD sem dados ali volta a escrever do zero enquanto a
/// pasta antiga continua no disco, intacta, para quem for olhar.
fn mudar_os_dados_do_legado(pacotes: &Path, dados: &Path) {
    let Ok(autores) = std::fs::read_dir(pacotes) else {
        return;
    };
    for autor in autores.flatten() {
        let Ok(nomes) = std::fs::read_dir(autor.path()) else {
            continue;
        };
        for nome in nomes.flatten() {
            let antiga = nome.path().join("dados");
            if !antiga.is_dir() {
                continue;
            }
            let Some(a) = autor.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some(n) = nome.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let nova = dados.join(&a).join(&n);
            if nova.exists() {
                continue;
            }
            if let Some(pai) = nova.parent() {
                if std::fs::create_dir_all(pai).is_err() {
                    continue;
                }
            }
            match std::fs::rename(&antiga, &nova) {
                Ok(()) => tracing::info!(
                    mod_id = %format!("{a}/{n}"),
                    "os arquivos deste MOD passaram para a raiz da instância"
                ),
                Err(erro) => tracing::warn!(
                    mod_id = %format!("{a}/{n}"),
                    %erro,
                    "não consegui mover os arquivos deste MOD; a pasta antiga continua no disco"
                ),
            }
        }
    }
}

#[cfg(test)]
mod os_servidores_guardados {
    use super::{banco, criar, listar, marcar_uso, raizes_dos_mods, renomear};

    /// Uma pasta de configuração vazia, só para este teste.
    fn pasta() -> tempfile::TempDir {
        tempfile::tempdir().expect("o sistema tem de dar um diretório temporário")
    }

    fn texto(dir: &tempfile::TempDir) -> String {
        dir.path().to_string_lossy().into_owned()
    }

    #[test]
    fn uma_maquina_nova_nao_tem_servidor_nenhum() {
        let dir = pasta();
        assert!(listar(&texto(&dir)).is_empty());
    }

    /// **O teste que a decisão inteira existe para sustentar.**
    ///
    /// Quem já hospedou tem um `seele.db` e nenhum registro. A primeira
    /// listagem tem de adotá-lo **onde ele está** — e o arquivo tem de
    /// continuar lá, com os bytes que tinha.
    #[test]
    fn o_servidor_que_ja_existia_e_adotado_sem_sair_do_lugar() {
        let dir = pasta();
        let config = texto(&dir);
        let antigo = dir.path().join("seele.db");
        std::fs::write(&antigo, b"as conversas de quem ja usava").expect("escrever o banco antigo");

        let lista = listar(&config);

        assert_eq!(lista.len(), 1, "o servidor de sempre entra na lista");
        let adotado = lista.first().expect("a lista tem um");
        assert!(
            adotado.caminho.is_none(),
            "adotado apontando para o banco de sempre, e não para um caminho novo"
        );
        assert_eq!(banco(&config, Some(&adotado.id)), antigo);
        assert_eq!(
            std::fs::read(&antigo).expect("o banco antigo continua legível"),
            b"as conversas de quem ja usava",
            "o arquivo não foi movido nem reescrito"
        );
    }

    #[test]
    fn adotar_acontece_uma_vez_e_nao_a_cada_listagem() {
        let dir = pasta();
        let config = texto(&dir);
        std::fs::write(dir.path().join("seele.db"), b"x").expect("escrever");

        listar(&config);
        listar(&config);

        assert_eq!(listar(&config).len(), 1, "sem duplicar o adotado");
    }

    #[test]
    fn um_servidor_novo_ganha_banco_proprio_e_nao_toca_no_antigo() {
        let dir = pasta();
        let config = texto(&dir);
        let antigo = dir.path().join("seele.db");
        std::fs::write(&antigo, b"antigo").expect("escrever");
        listar(&config);

        let novo = criar(&config, "Mesa de RPG", "0.11.0");
        let caminho = banco(&config, Some(&novo.id));

        assert_ne!(caminho, antigo, "dois servidores, dois bancos");
        assert!(
            caminho.starts_with(dir.path().join("servidores")),
            "o banco novo mora sob `servidores/`, e não solto na configuração"
        );
        assert!(
            caminho.parent().is_some_and(std::path::Path::is_dir),
            "a pasta do banco é criada antes de alguém tentar abri-lo"
        );
        assert_eq!(
            std::fs::read(&antigo).expect("o antigo continua lá"),
            b"antigo"
        );
        assert_eq!(listar(&config).len(), 2);
    }

    /// **O defeito relatado, e a prova de que pedir o banco já o conserta.**
    ///
    /// Sair de um servidor desta máquina e entrar em outro dava «A CHAVE DO
    /// SERVIDOR MUDOU»: o pino do TOFU é o endereço, e cada banco tinha a
    /// própria identidade. A herança mora dentro de `banco` justamente para
    /// que não exista um passo a esquecer — este teste não chama nada além do
    /// que `hospedar` chama.
    #[test]
    fn pedir_o_banco_de_um_servidor_novo_ja_planta_a_chave_desta_maquina() {
        use seele_server::persistence::{Location, Persistence};
        use seele_server::tls::{identidade_guardada, Identity};

        let dir = pasta();
        let config = texto(&dir);

        let legado = dir.path().join("seele.db");
        let velho = Persistence::open(&Location::File(legado)).expect("abrir o legado");
        Identity::load_or_create(&velho, vec!["localhost".to_owned()]).expect("identidade");
        let fixada = identidade_guardada(&velho).expect("o legado tem identidade");
        drop(velho);

        let novo = criar(&config, "Mesa de RPG", "0.11.0");
        let caminho = banco(&config, Some(&novo.id));

        let aberto = Persistence::open(&Location::File(caminho)).expect("abrir o novo");
        let identidade =
            Identity::load_or_create(&aberto, vec!["localhost".to_owned()]).expect("subir");
        assert_eq!(
            identidade.chain.first().map(|c| c.as_ref().to_vec()),
            Some(fixada.0),
            "o servidor novo apresenta a chave que as pessoas já fixaram"
        );
    }

    #[test]
    fn dois_servidores_com_o_mesmo_nome_nao_brigam_pela_mesma_pasta() {
        let dir = pasta();
        let config = texto(&dir);

        let um = criar(&config, "Casa", "0.11.0");
        let outro = criar(&config, "Casa", "0.11.0");

        assert_ne!(um.id, outro.id);
        assert_ne!(
            banco(&config, Some(&um.id)),
            banco(&config, Some(&outro.id))
        );
    }

    #[test]
    fn um_nome_sem_letra_nenhuma_ainda_da_um_identificador_usavel() {
        let dir = pasta();
        let config = texto(&dir);

        let s = criar(&config, "— ✳ —", "0.11.0");

        assert_eq!(s.id, "servidor");
        assert!(banco(&config, Some(&s.id)).starts_with(dir.path().join("servidores")));
    }

    /// Sem registro, ou com um id que não está nele, hospedar continua
    /// funcionando como sempre funcionou — e não com um erro sobre um arquivo
    /// que quem usa nunca soube que existia.
    #[test]
    fn um_id_desconhecido_cai_no_banco_de_sempre() {
        let dir = pasta();
        let config = texto(&dir);

        assert_eq!(banco(&config, None), dir.path().join("seele.db"));
        assert_eq!(
            banco(&config, Some("nunca-existiu")),
            dir.path().join("seele.db")
        );
    }

    #[test]
    fn a_lista_vem_do_mais_recente_para_o_mais_antigo() {
        let dir = pasta();
        let config = texto(&dir);
        let primeiro = criar(&config, "Um", "0.11.0");
        let segundo = criar(&config, "Dois", "0.11.0");

        // O relógio tem segundo inteiro; sem isto os dois empatam e a ordem
        // seria a de inserção por acaso, e não pelo uso.
        marcar_uso(&config, &primeiro.id);
        let mut lista = listar(&config);
        lista.sort_by_key(|s| std::cmp::Reverse(s.ultimo_uso));

        assert!(lista.iter().any(|s| s.id == segundo.id));
        assert_eq!(
            lista.first().map(|s| s.id.clone()),
            Some(primeiro.id),
            "o último usado vem primeiro"
        );
    }

    #[test]
    fn renomear_acerta_a_lista_e_nao_o_caminho() {
        let dir = pasta();
        let config = texto(&dir);
        let s = criar(&config, "Antes", "0.11.0");
        let caminho = banco(&config, Some(&s.id));

        renomear(&config, &s.id, "Depois");

        let lista = listar(&config);
        let achado = lista.iter().find(|x| x.id == s.id).expect("continua lá");
        assert_eq!(achado.nome, "Depois");
        assert_eq!(
            banco(&config, Some(&s.id)),
            caminho,
            "trocar o nome não muda onde os dados moram"
        );
    }

    #[test]
    fn um_registro_corrompido_nao_derruba_hospedar() {
        let dir = pasta();
        let config = texto(&dir);
        std::fs::write(dir.path().join("servidores.json"), b"{ isto nao e json")
            .expect("escrever lixo");

        // Não entra em pânico, e o caminho de sempre continua respondendo.
        assert!(listar(&config).is_empty());
        assert_eq!(banco(&config, None), dir.path().join("seele.db"));
    }

    /// **Dois servidores, o mesmo MOD, arquivos separados.**
    ///
    /// É o P0 do plano de isolamento: antes a raiz dos arquivos era a mesma
    /// para toda instância desta máquina, e o `dados/` saía de dentro do
    /// pacote. Um avatar gravado num servidor aparecia no outro, enquanto o
    /// `dados` chave-valor já estava separado, corretamente, em bancos
    /// diferentes — o que tornava o defeito mais confuso, e não menos.
    #[test]
    fn dois_servidores_com_o_mesmo_mod_nao_dividem_os_arquivos() {
        let dir = pasta();
        let config = texto(&dir);
        let a = criar(&config, "Casa A", "0.11.3").id;
        let b = criar(&config, "Casa B", "0.11.3").id;

        let ra = raizes_dos_mods(&config, Some(&a));
        let rb = raizes_dos_mods(&config, Some(&b));

        assert_ne!(
            ra.dados_de("seele/perfis"),
            rb.dados_de("seele/perfis"),
            "o mesmo MOD em dois servidores escreve no mesmo lugar"
        );
        // **E o pacote continua compartilhado, de propósito.** Ele é imutável e
        // conferido por hash: dois servidores que exigem os mesmos bytes
        // exigem os mesmos bytes, e duplicá-los seria gastar disco para nada.
        assert_eq!(
            ra.pacote_de("seele/perfis"),
            rb.pacote_de("seele/perfis"),
            "o pacote foi duplicado por instância; ele é imutável e não precisa"
        );
    }

    /// **O legado tem dados de verdade, e eles não podem ficar órfãos.**
    ///
    /// Toda máquina que hospedou antes de haver lista tem perfis e avatares em
    /// `mods/<autor>/<nome>/dados`. Trocar a raiz sem mais nada abriria o
    /// servidor com tudo em branco — que quem usa lê como perda, e com razão.
    #[test]
    fn os_arquivos_do_legado_passam_para_a_raiz_nova_sem_sumir() {
        let dir = pasta();
        let config = texto(&dir);
        let antiga = dir.path().join("mods/seele/perfis/dados");
        std::fs::create_dir_all(&antiga).expect("pasta antiga");
        std::fs::write(antiga.join("avatar.txt"), b"o avatar de alguem").expect("dado antigo");

        let raizes = raizes_dos_mods(&config, None);
        let agora = raizes.dados_de("seele/perfis").join("avatar.txt");

        assert_eq!(
            std::fs::read(&agora).expect("o dado tem de estar na raiz nova"),
            b"o avatar de alguem",
            "o conteúdo mudou na mudança"
        );
        assert!(
            !antiga.exists(),
            "a pasta antiga ficou para trás: a próxima leitura veria as duas e \
             não saberia qual vale"
        );
    }

    /// E pedir a raiz de novo não desfaz nem duplica: a mudança é uma vez.
    #[test]
    fn a_mudanca_do_legado_acontece_uma_vez_e_nao_atrapalha_depois() {
        let dir = pasta();
        let config = texto(&dir);
        let antiga = dir.path().join("mods/seele/perfis/dados");
        std::fs::create_dir_all(&antiga).expect("pasta antiga");
        std::fs::write(antiga.join("a.txt"), b"um").expect("dado");

        let raizes = raizes_dos_mods(&config, None);
        // Alguém escreve depois da mudança, como o MOD faria.
        std::fs::write(raizes.dados_de("seele/perfis").join("b.txt"), b"dois").expect("dado novo");
        // E a pasta antiga reaparece vazia, como um pacote reinstalado a deixa.
        std::fs::create_dir_all(&antiga).expect("pasta antiga de novo");

        let de_novo = raizes_dos_mods(&config, None);
        assert_eq!(
            de_novo.dados_de("seele/perfis"),
            raizes.dados_de("seele/perfis")
        );
        assert!(
            de_novo.dados_de("seele/perfis").join("b.txt").is_file(),
            "a segunda chamada levou a pasta vazia por cima do que havia"
        );
        assert!(
            de_novo.dados_de("seele/perfis").join("a.txt").is_file(),
            "o que veio do legado sumiu na segunda chamada"
        );
    }
}
