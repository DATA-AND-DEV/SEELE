//! O motor: o que acontece, na ordem em que acontece.
//!
//! **Fora da janela de propósito.** Há três caminhos até aqui — a janela, o modo
//! passivo e o silencioso —, e os três têm de fazer exatamente a mesma coisa. Um
//! motor dentro da janela seria um motor que só o primeiro caminho exercita, e
//! os outros dois são justamente os que ninguém olha.
#![cfg(windows)]

use std::path::{Path, PathBuf};

use crate::{carga, registro, sistema};

/// O que quem instala escolheu.
///
/// Duas, as que o produto sabe cumprir. Ver o comentário de `OPCOES` em
/// `janela.rs` sobre as outras duas do desenho.
#[derive(Clone, Copy)]
pub(crate) struct Escolhas {
    /// Atalho na área de trabalho e no menu Iniciar.
    pub(crate) atalho: bool,
    /// Regra de entrada da 8383 no firewall.
    pub(crate) porta: bool,
}

impl Escolhas {
    /// O que vale quando ninguém respondeu — numa atualização silenciosa.
    ///
    /// **Lidas do registro, e não presumidas.** A atualização roda sem tela: se
    /// ela usasse um padrão fixo, quem tinha a porta aberta a perderia numa
    /// atualização que não pediu, e o servidor pararia de aceitar conexão sem
    /// nada explicar.
    ///
    /// Sem valor gravado — quem instalou antes desta versão — o padrão é o
    /// comportamento antigo do instalador NSIS: atalho sim, porta sim. Mudar o
    /// padrão por baixo de quem já hospeda seria a mesma quebra por outro
    /// caminho.
    pub(crate) fn de_antes() -> Self {
        let (atalho, porta) = registro::escolhas_guardadas();
        Self {
            atalho: atalho.unwrap_or(true),
            porta: porta.unwrap_or(true),
        }
    }
}

/// A pasta que o instalador propõe, ou aquela onde o SEELE já está.
///
/// **A de antes ganha.** Numa atualização, instalar noutro lugar deixaria duas
/// cópias e um atalho apontando para a velha — que é exatamente o defeito que a
/// 0.7.2 levou meses para descobrir.
pub(crate) fn pasta_padrao() -> String {
    if let Some(ja_instalado) = registro::onde_esta_instalado() {
        return ja_instalado;
    }
    let base = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_owned());
    format!(r"{base}\SEELE")
}

/// Instala, do começo ao fim.
///
/// `contar` recebe cada passo em português, para o log do passo 03 e para a
/// saída do modo silencioso. Ele é chamado **depois** de cada coisa dar certo,
/// nunca antes: um log que anuncia o que vai tentar mente quando a tentativa
/// falha.
///
/// # Errors
///
/// Devolve o primeiro passo que não deu certo, com o que o Windows respondeu.
pub(crate) fn executar(
    destino: &Path,
    escolhas: Escolhas,
    contar: &dyn Fn(&str),
) -> Result<(), String> {
    // **Antes de escrever o primeiro arquivo.** O Windows não deixa sobrescrever
    // um executável em uso, e descobrir isso no meio deixaria parte dos arquivos
    // novos e parte velhos — o estado mais difícil de explicar.
    if !sistema::windows_novo_o_bastante() {
        return Err(
            "este Windows é anterior ao 10, e o SEELE não abre nele: o runtime \
             do WebView2, que o app usa para desenhar, não existe para versões \
             mais antigas.\n\
             Nada foi escrito nesta máquina."
                .to_owned(),
        );
    }

    // **A arquitetura não é conferida, e isso é uma decisão e não um esquecimento.**
    //
    // Este instalador é um executável de 64 bits: num Windows de 32 bits ele não
    // chega a rodar — o próprio sistema o recusa antes, com a mensagem dele. Um
    // teste aqui seria código que nunca pode reprovar, e código que nunca reprova
    // é código que ninguém sabe se funciona.

    if sistema::produto_aberto() {
        return Err("o SEELE está aberto nesta máquina. Feche-o e instale de novo.".to_owned());
    }

    let quantos = carga::abrir_em(destino, |arquivo| contar(arquivo))?;
    contar(&format!("{quantos} arquivos escritos"));

    // **O produto tem de estar lá.**
    //
    // Uma carga que extrai sem erro e não traz o executável deixa a pasta cheia
    // de arquivos, o atalho apontando para o vazio e a entrada no painel
    // anunciando um programa que não existe — tudo isso *com sucesso*. É o pior
    // resultado possível, e custa uma linha impedir.
    let produto = destino.join("SEELE.exe");
    if !produto.is_file() {
        return Err(format!(
            "a carga foi extraída e o `SEELE.exe` não está em {}.\n\
             O instalador não vai criar atalho nem registrar nada apontando para \
             um programa que não existe.",
            destino.display()
        ));
    }

    // O desinstalador **antes** da entrada que aponta para ele: uma entrada que
    // aponta para um arquivo inexistente não desinstala, e ninguém descobre isso
    // na instalação — só meses depois, quando alguém tenta remover.
    let eu = std::env::current_exe()
        .map_err(|erro| format!("não sei onde este programa está: {erro}"))?;
    let desinstalador = destino.join("desinstalar.exe");
    std::fs::copy(&eu, &desinstalador).map_err(|erro| {
        format!(
            "não copiei o desinstalador para {}: {erro}",
            desinstalador.display()
        )
    })?;
    contar("desinstalador escrito");

    registro::anunciar(
        &destino.display().to_string(),
        env!("CARGO_PKG_VERSION"),
        &desinstalador.display().to_string(),
        tamanho_no_disco(destino),
    )?;
    registro::guardar_escolhas(escolhas.atalho, escolhas.porta)?;
    contar("registrado em «Aplicativos instalados»");

    if escolhas.atalho {
        if let Some(menu) = sistema::menu_iniciar() {
            sistema::atalho(
                &menu.join("SEELE.lnk"),
                &produto,
                "Voz, vídeo e texto auto-hospedados",
            )?;
            contar("atalho no menu Iniciar");
        }
        if let Some(mesa) = sistema::area_de_trabalho() {
            sistema::atalho(
                &mesa.join("SEELE.lnk"),
                &produto,
                "Voz, vídeo e texto auto-hospedados",
            )?;
            contar("atalho na área de trabalho");
        }
    }

    sistema::regra_de_firewall(&produto, escolhas.porta)?;
    // Os perfis nomeados, e não só «aberta»: a regra **não** vale em rede
    // pública, e quem hospeda de uma precisa saber disso por aqui em vez de
    // descobrir por ninguém conseguir entrar. Ver `sistema::regra_de_firewall`.
    contar(if escolhas.porta {
        "porta 8383 UDP aberta no firewall, em rede de domínio e privada — não em rede pública"
    } else {
        "firewall: regra não pedida"
    });

    // **O WebView2 depois de o produto estar inteiro, e sem poder de veto.**
    //
    // Ele precisa de rede e pode demorar; falhar aqui não desfaz nada — o SEELE
    // fica instalado e abre uma janela em branco até o runtime existir. Por isso
    // o erro é **contado e seguido**, não devolvido: derrubar a instalação
    // inteira porque a rede caiu no último passo desfaria o que já deu certo.
    if sistema::webview2_instalado() {
        contar("WebView2: já estava aqui");
    } else {
        contar("WebView2 ausente; baixando da Microsoft");
        match sistema::instalar_webview2() {
            Ok(()) => contar("WebView2 instalado"),
            Err(motivo) => contar(&format!("WebView2: {motivo}")),
        }
    }

    // **Por último**, e só depois de a nova estar de pé: apagar a antiga antes
    // deixaria a máquina sem SEELE nenhum se o que vem depois falhasse.
    if let Some(antiga) = sistema::instalacao_por_usuario() {
        if std::fs::remove_dir_all(&antiga).is_ok() {
            contar("instalação antiga por usuário removida");
        } else {
            contar("instalação antiga por usuário: não consegui apagar");
        }
    }

    Ok(())
}

/// Abre o produto recém-instalado, como o usuário e não como administrador.
///
/// **Como o usuário, e isto não é detalhe.** O instalador roda elevado; um filho
/// dele nasceria elevado também, e o SEELE passaria a gravar identidade, pinos e
/// preferências na pasta do administrador — onde a próxima abertura normal não
/// os acha. A pessoa perderia a chave dela numa atualização.
///
/// # Sem argumentos, e é o `explorer.exe` que manda
///
/// Ela recebia uma lista de argumentos e os punha na linha do Explorer. **O
/// Explorer não encaminha argumento nenhum**: ele recebe um item para abrir, e o
/// resto ele tenta abrir também. Com `/UPDATE /ARGS` na linha — que é o que o
/// atualizador do Tauri sempre manda — o produto não abria.
///
/// Relatado assim: «ele não reabre o app após atualizar». O botão ABRIR O SEELE
/// da janela sempre funcionou, e a diferença entre os dois caminhos era esta
/// lista: vazia num, com dois argumentos no outro. A mesma função, dois
/// resultados.
///
/// **E não há o que perder.** O `seele-app` não lê `argv` em lugar nenhum — os
/// argumentos que o atualizador reenvia são os que o app tinha, e ele não os
/// consulta. No dia em que passar a consultar, baixar o privilégio deixa de
/// poder ser o Explorer: será preciso o token da shell —
/// `GetShellWindow`/`OpenProcessToken`/`CreateProcessWithTokenW` — que
/// encaminha linha de comando e é bem mais código.
pub(crate) fn abrir_o_produto(destino: &Path) {
    let produto = destino.join("SEELE.exe");
    if !produto.is_file() {
        return;
    }
    // `explorer.exe <programa>` é o jeito documentado de baixar o privilégio: o
    // Explorer roda como o usuário, e o que ele lança herda o token dele.
    let _ = std::process::Command::new("explorer.exe")
        .arg(&produto)
        .spawn();
}

/// Quanto a pasta ocupa, em KiB — que é a unidade do `EstimatedSize`.
///
/// Medido depois de escrever, e não estimado da carga comprimida: o painel do
/// Windows mostra este número, e um número inventado ali é informação errada
/// numa tela do sistema.
fn tamanho_no_disco(pasta: &Path) -> u32 {
    fn somar(pasta: &Path, total: &mut u64) {
        let Ok(itens) = std::fs::read_dir(pasta) else {
            return;
        };
        for item in itens.flatten() {
            let Ok(tipo) = item.file_type() else { continue };
            if tipo.is_dir() {
                somar(&item.path(), total);
            } else if let Ok(dados) = item.metadata() {
                *total += dados.len();
            }
        }
    }
    let mut total = 0_u64;
    somar(pasta, &mut total);
    u32::try_from(total / 1024).unwrap_or(u32::MAX)
}

/// O caminho da instalação como `PathBuf`, para quem tem só o texto.
pub(crate) fn como_caminho(texto: &str) -> PathBuf {
    PathBuf::from(texto)
}
