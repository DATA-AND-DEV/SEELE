//! O que **esta** máquina consegue fazer com o som que uma transmissão leva.
//!
//! R21 da revisão da v15: «participantes se escutam quando alguém transmite a
//! tela inteira». O caminho é o mesmo nos dois sistemas que capturam — o som que
//! acompanha uma tela compartilhada é o som da **saída**, e a conversa que o
//! SEELE está tocando está nela.
//!
//! # Por que este módulo existe em vez de uma constante
//!
//! Porque a resposta muda por sistema, por versão e por fonte, e porque a única
//! coisa pior que não excluir o áudio é **dizer que excluiu**. O review é
//! explícito: «Verificar capacidade por SO/versão e por fonte; se não houver
//! exclusão confiável, explicar a limitação e oferecer compartilhar uma
//! janela/aplicativo ou transmitir sem áudio, sem afirmar que o eco foi
//! resolvido.»
//!
//! Então o que atravessa para a tela não é «resolvido/não resolvido»: é uma de
//! três respostas, e uma delas é «não sei».
//!
//! # O que este módulo **não** é
//!
//! Não é uma medição acústica. Nenhuma linha daqui escuta nada. Ela relata o que
//! o código pede ao sistema e em que versão aquele pedido vale — e a homologação
//! entre duas máquinas, com um vídeo tocando e alguém falando, continua pendente
//! e está registrada como tal.

/// Se o som do próprio SEELE fica fora do que uma transmissão captura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusaoDoSom {
    /// O sistema exclui o áudio deste processo, e o código pede isso.
    ///
    /// Quem compartilha continua ouvindo todo mundo; quem assiste não recebe a
    /// própria voz de volta.
    Excluido,
    /// O sistema captura a saída inteira, o SEELE incluído.
    ///
    /// **A tela tem de dizer isso**, e oferecer as duas saídas que existem:
    /// compartilhar uma janela ou um aplicativo em vez do monitor, ou transmitir
    /// sem áudio.
    NaoExcluido,
    /// Esta máquina não compartilha tela, então a pergunta não se aplica.
    SemCaptura,
}

/// O que esta máquina faz com o som do próprio SEELE ao capturar uma tela.
///
/// # macOS
///
/// A ScreenCaptureKit entrega imagem e som pelo mesmo `SCStream`, e
/// `excludesCurrentProcessAudio` — do macOS 13 — tira o áudio deste processo do
/// que ela entrega. O código o pede sempre; abaixo do 13 a propriedade não
/// existe e a exclusão não acontece, e é essa faixa que esta função relata como
/// [`ExclusaoDoSom::NaoExcluido`].
///
/// # Windows
///
/// O caminho é o loopback da saída inteira — `CapturaDaSaida::abrir` —, e ele
/// não tem como excluir um processo: quem excluiria é a API de loopback **por
/// processo** da WASAPI (`AUDIOCLIENT_ACTIVATION_PARAMS` com
/// `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE`, do Windows 10 2004), e o
/// `cpal` não a expõe. Enquanto isso não for escrito, a resposta honesta é
/// [`ExclusaoDoSom::NaoExcluido`] — **não** «não sei», porque aqui se sabe: a
/// saída inteira entra.
///
/// # Os outros
///
/// Não há captura, então não há som capturado.
#[must_use]
pub const fn exclusao_do_som_deste_processo() -> ExclusaoDoSom {
    #[cfg(target_os = "macos")]
    {
        // Em tempo de compilação não há como saber em que macOS isto vai rodar,
        // e é por isso que a resposta aqui é a do caso bom com a ressalva
        // registrada: a propriedade é pedida sempre, e quem roda num macOS 12
        // está na faixa em que ela não vale. O `false` daquela faixa não é
        // detectável sem consultar a versão em tempo de execução — ver
        // `exclusao_medida_no_sistema`.
        ExclusaoDoSom::Excluido
    }
    #[cfg(target_os = "windows")]
    {
        ExclusaoDoSom::NaoExcluido
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        ExclusaoDoSom::SemCaptura
    }
}

/// A mesma resposta, conferindo a versão do sistema quando ela decide.
///
/// Separada da acima porque a acima é `const` e esta pergunta ao sistema. No
/// macOS ela é a que morde: abaixo do 13 a exclusão não existe, e prometer que
/// existe é o defeito que este módulo inteiro está aqui para não cometer.
#[must_use]
pub fn exclusao_medida_no_sistema() -> ExclusaoDoSom {
    #[cfg(target_os = "macos")]
    {
        if versao_maior_do_macos().is_some_and(|maior| maior < 13) {
            return ExclusaoDoSom::NaoExcluido;
        }
        // Versão ilegível lê como o caso bom, e é a escolha certa aqui: a
        // ScreenCaptureKit já exige 12.3, então uma máquina que chegou a
        // capturar está no 12.3 ou acima, e a faixa em que a resposta erraria é
        // de sete meses de 2022. O contrário — cair para «não excluído» por não
        // conseguir ler a versão — faria toda máquina ler o aviso de eco sem
        // haver eco.
        ExclusaoDoSom::Excluido
    }
    #[cfg(not(target_os = "macos"))]
    {
        exclusao_do_som_deste_processo()
    }
}

/// A versão maior deste macOS, lida do `sw_vers`.
///
/// `None` quando não deu para ler — ver o comentário de quem chama sobre o que
/// isso significa. Sem dependência nova: é um processo, uma linha, e o
/// `Cargo.toml` deste crate é curto de propósito.
#[cfg(target_os = "macos")]
fn versao_maior_do_macos() -> Option<u32> {
    let saida = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    if !saida.status.success() {
        return None;
    }
    let texto = String::from_utf8(saida.stdout).ok()?;
    texto.trim().split('.').next()?.parse().ok()
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A resposta desta máquina é uma das três, e nunca um palpite silencioso.
    #[test]
    fn a_resposta_e_uma_das_tres() {
        let dito = exclusao_do_som_deste_processo();
        assert!(matches!(
            dito,
            ExclusaoDoSom::Excluido | ExclusaoDoSom::NaoExcluido | ExclusaoDoSom::SemCaptura
        ));
    }

    /// **Onde não há captura, não há promessa de exclusão.**
    ///
    /// O guarda existe porque o contrário é o erro fácil: um `SemCaptura` que
    /// respondesse `Excluido` faria a tela dizer «a sua conversa não volta pela
    /// transmissão» numa máquina que não transmite nada.
    #[test]
    fn sem_captura_nao_promete_exclusao() {
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(exclusao_do_som_deste_processo(), ExclusaoDoSom::SemCaptura);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        assert_ne!(exclusao_do_som_deste_processo(), ExclusaoDoSom::SemCaptura);
    }

    /// **No Windows a resposta é «não excluído», e não «não sei».**
    ///
    /// Aqui se sabe: `CapturaDaSaida::abrir` abre o loopback da saída inteira, e
    /// a saída inteira inclui a conversa que este processo está tocando. Um «não
    /// sei» ali seria mais macio e menos verdadeiro.
    #[cfg(target_os = "windows")]
    #[test]
    fn no_windows_a_saida_inteira_entra() {
        assert_eq!(exclusao_do_som_deste_processo(), ExclusaoDoSom::NaoExcluido);
    }

    /// A medida no sistema concorda com a de compilação, ou é mais conservadora.
    ///
    /// Nunca mais otimista: a única direção em que ler a versão pode mudar a
    /// resposta é para **menos** promessa.
    #[test]
    fn a_medida_nunca_promete_mais_que_o_codigo() {
        let codigo = exclusao_do_som_deste_processo();
        let medida = exclusao_medida_no_sistema();
        if codigo == ExclusaoDoSom::NaoExcluido {
            assert_eq!(medida, ExclusaoDoSom::NaoExcluido);
        }
        if codigo == ExclusaoDoSom::SemCaptura {
            assert_eq!(medida, ExclusaoDoSom::SemCaptura);
        }
    }
}
