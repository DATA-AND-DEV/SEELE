//! A casa comum de quem precisa **subir de novo na mesma porta**.
//!
//! Dois testes desta suíte derrubam o servidor e o levantam de volta no mesmo
//! endereço, e os dois tropeçavam na mesma pedra. Ela não é do produto: é do
//! sorteio de portas do sistema. Este módulo existe para que a resposta a ela
//! seja uma só — corrigir aqui corrige nos dois, e ninguém precisa descobrir
//! duas vezes por que a espera existe.
//!
//! **Por que só dois.** Estes são os únicos binários da conformidade que pedem
//! uma porta fixa; todos os outros pedem porta efêmera ao sistema e não têm como
//! tropeçar em «endereço já em uso». A cobertura deste módulo é de dois arquivos
//! porque a exposição é de dois arquivos, e não porque os demais ficaram para
//! trás — conferido em 15/09/2026, lendo todo `listen:` da suíte.
//!
//! Não é um binário de teste: `cargo` só compila como teste os arquivos soltos
//! em `tests/`, e não o que está dentro de uma pasta. Quem usa faz `mod porta;`.
//!
//! A prova de que a insistência serve para alguma coisa mora em
//! `tela_por_um_par.rs`, no teste
//! `a_porta_tomada_por_um_instante_nao_reprova_a_volta_do_servidor`: ele encena
//! a disputa de propósito e reprova se `ligar_insistindo` virar `Daemon::bind`.

use std::time::Duration;

use anyhow::Result;
use seele_server::{Daemon, ServerConfig};

/// Liga o daemon, insistindo enquanto **a porta ainda estiver de outro dono**.
///
/// Medido em 14/09, rodando a suíte inteira: entre a queda e a volta a porta
/// fica livre por um instante, e outro binário de teste — que pede porta
/// efêmera com `bind(0)` — chega a levá-la. O `bind` de volta devolve
/// `Address already in use`, e o teste reprova por causa do sorteio de portas
/// do sistema, não do produto. Essa foi a reprovação que travou a integração
/// desta entrega duas vezes, em testes diferentes a cada rodada.
///
/// Insistir por alguns segundos é o que um operador faria ao subir de novo um
/// servidor que acabou de cair, e só para a espera de quem tem motivo: outro
/// erro qualquer sobe na hora, sem esconder defeito atrás de repetição.
pub(crate) async fn ligar_insistindo(config: ServerConfig) -> Result<Daemon> {
    const ESPERA: Duration = Duration::from_millis(100);
    const TETO: Duration = Duration::from_secs(10);

    let comeco = std::time::Instant::now();
    loop {
        match Daemon::bind(config.clone()).await {
            Ok(daemon) => return Ok(daemon),
            Err(erro) if a_porta_e_de_outro(&erro) && comeco.elapsed() < TETO => {
                tokio::time::sleep(ESPERA).await;
            }
            Err(erro) => return Err(erro),
        }
    }
}

/// Se a recusa foi «a porta já tem dono», e não outra coisa.
pub(crate) fn a_porta_e_de_outro(erro: &anyhow::Error) -> bool {
    erro.chain().any(|causa| {
        causa
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::AddrInUse)
    })
}
