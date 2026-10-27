//! **Um servidor para as bancadas do navegador** (Tarefa 2.6 da F4): hospedado
//! como o app hospeda (`Hospedagem::iniciar`), com o banco numa pasta
//! temporária.
//!
//!     cargo run -p seele-server --example servidor_da_bancada [--local] [--proprietario <hex64>] [pasta-de-um-mod]
//!
//! **`--local`**: o modo local (E9, `Hospedagem::iniciar_local`). Atende só em
//! `127.0.0.1` e não sobe a escada, sem UPnP, PCP nem ponto de encontro. A
//! bancada não depende da LAN nem do roteador, e o `endereco` sai em
//! `127.0.0.1`. Sem a opção, o exemplo hospeda como o app e pode abrir porta
//! no roteador real. A saída ganha `"local"`.
//!
//! **`--proprietario <hex64>`**, igual ao do `seeled` (ADR 0064): declara o dono
//! pela impressão da chave dele, antes de aceitar a primeira conexão. Sem dono
//! ninguém tem GERIR CARGOS (o assento por chegada saiu na 0.17), e a bancada
//! de cargos não teria quem os gerisse. Uma impressão que não é hex de 64
//! caracteres faz o exemplo sair com erro, sem subir. A saída ganha
//! `"proprietario"` (`null` sem a opção).
//!
//! A primeira linha da saída é um JSON com o que a bancada precisa para entrar
//! pela página: `{"endereco", "impressao", "link"}`. O `"ate"` saiu na 0.17: o link
//! do navegador não vence mais (sem certificados curtos).
//!
//! **O link é o que a tela de quem hospeda mostra** (D-web-6): o convite que o
//! `hospedar` do app devolve sem nome público (`convite_com_nome(None)`),
//! passado ao `Hospedagem::link_do_navegador` — o mesmo que o comando
//! `link_do_navegador` da casca chama. Até a D-web-6 este exemplo montava o
//! link à mão, com as janelas que valiam agora; a bancada provava um link que o
//! produto não dava a ninguém. `endereco` é o alvo desse convite: o endereço da
//! rede desta máquina que a escada achou, e não `127.0.0.1`.
//!
//! O servidor fica de pé até a entrada padrão fechar: a bancada o encerra
//! fechando o tubo, e um processo esquecido não segura a porta. Ao sair ele
//! encerra a hospedagem, que devolve a porta do roteador, se a escada abriu
//! uma.
//!
//! **Com um MOD** (a pasta do pacote, a que tem o `mod.json`): o servidor o
//! exige de quem entra, como o `servidor-com-mod` — instalado por hash
//! (`instalar_de_pasta`) e ligado antes da subida, para a subida carregar a
//! metade de servidor. A saída ganha `"mod": {"id", "version", "hash"}`.
//!
//! **Não é o `seeled`**, nem o app inteiro: sem arquivo de configuração, sem
//! ponto de encontro (`iniciar`, e não o `iniciar_com_o_ponto_padrao` do
//! botão HOSPEDAR), e com a portaria desligada, como num banco novo.

#![allow(clippy::print_stdout, reason = "a saída é o contrato com a bancada")]

use anyhow::Result;
use seele_server::hospedagem::Hospedagem;
use seele_server::mods::instalar::{instalar_de_pasta, raizes_de_um_mundo};
use seele_server::persistence::mods::{enable, EnabledMod};
use seele_server::persistence::{Location, Persistence};
use std::io::Read as _;

#[tokio::main]
async fn main() -> Result<()> {
    // O banco e os anexos numa pasta própria, apagada ao sair: os anexos
    // nascem ao lado do banco, e sem eles o servidor recusa todo anexo com
    // `Unavailable`, e a bancada não provaria a subida.
    let pasta = std::env::temp_dir().join(format!("servidor-da-bancada-{}", std::process::id()));
    let mundo = pasta.join("mundo");
    std::fs::create_dir_all(&mundo)?;
    let banco = Location::File(mundo.join("seele.db"));
    let mut exigido = serde_json::Value::Null;
    let mut raizes = None;
    let mut proprietario: Option<String> = None;
    let mut origem = None;
    let mut local = false;
    let mut argumentos = std::env::args_os().skip(1);
    while let Some(argumento) = argumentos.next() {
        if argumento == "--proprietario" {
            let impressao = argumentos
                .next()
                .ok_or_else(|| {
                    anyhow::anyhow!("`--proprietario` precisa da impressão (hex64) do dono")
                })?
                .into_string()
                .map_err(|_| anyhow::anyhow!("`--proprietario`: a impressão não é texto"))?;
            proprietario = Some(impressao);
        } else if argumento == "--local" {
            local = true;
        } else {
            origem = Some(argumento);
        }
    }
    if let Some(origem) = origem {
        let dos_mods = raizes_de_um_mundo(&mundo);
        let (manifesto, hash) = instalar_de_pasta(std::path::Path::new(&origem), &dos_mods)?;
        enable(
            &Persistence::open(&banco)?,
            &EnabledMod {
                id: manifesto.id.clone(),
                version: manifesto.version.clone(),
                hash: hash.clone(),
                repo: manifesto.repo.clone(),
                reach: manifesto.reach.clone(),
                server_half: manifesto.server.is_some(),
                api: manifesto.api,
                tema: false,
            },
        )?;
        exigido =
            serde_json::json!({ "id": manifesto.id, "version": manifesto.version, "hash": hash });
        raizes = Some(dos_mods);
    }
    let dono = proprietario.clone();
    let antes_de_aceitar = move |persistence: &mut Persistence| {
        if let Some(impressao) = &dono {
            seele_server::cargos::declarar_proprietario(persistence, impressao)?;
        }
        Ok(())
    };
    let hospedagem = if local {
        Hospedagem::iniciar_local(0, banco, "Bancada", raizes, antes_de_aceitar).await?
    } else {
        Hospedagem::iniciar(0, banco, "Bancada", raizes, antes_de_aceitar).await?
    };
    let convite = hospedagem.convite_com_nome(None);
    let link = hospedagem
        .link_do_navegador(&convite)
        .map_err(|sem| anyhow::anyhow!("o link do navegador não saiu: {sem:?}"))?;
    let alvo = seele_proto::uri::analisar(&convite)
        .map_err(|erro| anyhow::anyhow!("o convite não se lê: {erro:?}"))?
        .alvo;
    println!(
        "{}",
        serde_json::json!({
            "endereco": alvo,
            "impressao": hospedagem.impressao_digital(),
            "link": link.link,
            "mod": exigido,
            "proprietario": proprietario,
            "local": local,
        })
    );

    // Até a bancada fechar o tubo.
    tokio::task::spawn_blocking(|| {
        let mut resto = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut resto);
    })
    .await?;
    hospedagem.encerrar().await;
    let _ = std::fs::remove_dir_all(&pasta);
    Ok(())
}
