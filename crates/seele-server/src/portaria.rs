//! Quem bate à porta, e quem decide. ADR 0030.
//!
//! A terceira camada de admissão, e a única que decide sobre **gente**.
//! `admissao` decide sobre um segredo: quem sabe a senha ou traz um convite
//! passa, e o servidor não pergunta quem é. Aqui pergunta.
//!
//! # Por que isto cabe neste produto
//!
//! É TOFU (ADR 0003) virado do avesso. Lá, quem entra fixa a chave de quem
//! hospeda no primeiro contato; aqui, quem hospeda decide sobre a chave de quem
//! entra no primeiro contato, e não se pergunta de novo. Só dá para fazer isso
//! porque já existe identidade durável por pessoa — chave Ed25519 em disco (ADR
//! 0004), apelido preso à chave (ADR 0017). Não há cadastro nenhum sendo
//! inventado: a pergunta «é a primeira vez que esta pessoa aparece?» este
//! produto já sabia responder e não usava para nada.
//!
//! # Depois do desafio-resposta, e este é o desenho
//!
//! `admissao` roda no `Hello`, antes da assinatura, para não gastar verificação
//! com quem varre a internet. Esta roda **depois**, e a inversão é deliberada:
//! fixar uma impressão digital que ninguém provou não é TOFU, é fixar um
//! palpite. Qualquer um poderia encher a fila com chaves alheias, e quem
//! hospeda aprovaria uma pessoa e admitiria outra.
//!
//! O que impede a fila de encher com chaves *próprias*, que são de graça de
//! gerar, não está aqui: é o balde por endereço do ADR 0025, que responde antes
//! do `Hello` ser lido. As camadas compõem — cada uma cobre o flanco que a
//! anterior deixou aberto de propósito.
//!
//! # Nada espera
//!
//! Um pedido pendente derruba a conexão na hora, com `AdmissionPending`. Segurar
//! a conexão obrigaria a um prazo, e um prazo fabrica a resposta «ninguém
//! atendeu», que quem a recebe não sabe o que fazer com ela. Um pedido durável
//! que quem hospeda concede horas depois é promessa mais forte que uma barra
//! girando — e não custa recurso do servidor por alguém que ainda não entrou, que é
//! o mesmo argumento de `admissao`.
//!
//! É também a resposta a «e se ninguém estiver olhando»: o pedido é uma linha em
//! SQLite. Sobrevive à janela minimizada, ao app fechado e à máquina
//! reiniciada. Nada é recusado por omissão; é adiado, e o adiamento é durável.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use crate::persistence::{now_seconds, Persistence};

/// A chave de configuração que liga a portaria.
const CHAVE: &str = "portaria";

/// O que a portaria respondeu sobre uma chave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resposta {
    /// Entra. Ou já foi admitido, ou a portaria está desligada.
    Entra,
    /// O pedido está registrado e ninguém decidiu ainda.
    Pendente,
    /// Quem hospeda decidiu que não.
    Recusado,
}

/// Com que segredo alguém chegou à porta.
///
/// Prova exibida a quem decide, e nunca decisão por si: o ADR 0030 recusa
/// aprovar sozinho quem traz convite válido, porque um link se encaminha e
/// porque isso transformaria a camada mais forte na mais fraca.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segredo {
    /// O servidor não pedia nenhum.
    Aberto,
    /// A senha do servidor.
    Senha,
    /// Um convite de uso único, que esta batida gastou.
    Convite,
}

impl Segredo {
    /// O nome que vai para o banco e sai dele.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Aberto => "aberto",
            Self::Senha => "senha",
            Self::Convite => "convite",
        }
    }
}

/// Um pedido na fila, como quem hospeda o vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    /// SHA-256 da chave pública. **A identidade**, e a primeira linha do cartão.
    pub impressao: String,
    /// O apelido que a pessoa pediu. Texto que ela digitou, e nada além disso.
    pub apelido: String,
    /// Com que segredo chegou.
    pub segredo: String,
    /// A observação do convite, quando veio por um.
    pub observacao: String,
    /// Quando bateu pela primeira vez.
    pub bateu_em: i64,
    /// Quantas vezes bateu.
    pub batidas: i64,
    /// `None` enquanto pendente.
    pub decidido_em: Option<i64>,
    /// `true` se a decisão foi admitir.
    pub admitido: bool,
}

/// Se este servidor pergunta antes de deixar entrar quem nunca entrou.
///
/// Ausente significa desligada: um servidor que já existia não muda de
/// comportamento por ter sido migrado.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn ligada(persistence: &Persistence) -> Result<bool> {
    let valor: Option<String> = persistence
        .connection()
        .query_row(
            "SELECT valor FROM configuracao WHERE chave = ?1",
            params![CHAVE],
            |linha| linha.get(0),
        )
        .optional()?;
    Ok(valor.as_deref() == Some("ligada"))
}

/// Liga ou desliga a portaria.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn ligar(persistence: &mut Persistence, ligada: bool) -> Result<()> {
    persistence.connection().execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
         ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
        params![CHAVE, if ligada { "ligada" } else { "desligada" }],
    )?;
    Ok(())
}

/// Liga a portaria **se ninguém tiver decidido nada sobre ela ainda**.
///
/// A semente do botão HOSPEDAR AQUI. O ADR 0021 mantém o padrão aberto porque é
/// o que faz o teste em rede local funcionar sem cerimônia, e isso continua
/// valendo para o `seeled`; quem apertou um botão não aceitou cerimônia
/// nenhuma, e para ele o padrão é perguntar.
///
/// `INSERT` que não sobrescreve, e não `ligar(true)`, porque isto roda **toda
/// vez** que o app sobe um servidor. Quem desligou a portaria de propósito não
/// pode vê-la voltar sozinha na próxima abertura da janela — um interruptor que
/// se rearma é um interruptor quebrado.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn semear_ligada(persistence: &mut Persistence) -> Result<()> {
    persistence.connection().execute(
        "INSERT OR IGNORE INTO configuracao (chave, valor) VALUES (?1, 'ligada')",
        params![CHAVE],
    )?;
    Ok(())
}

/// Com que segredo alguém acabou de passar por `admissao`, para o cartão.
///
/// Lida **depois** de `Politica::admitir` ter dito que sim, e por isso não
/// decide nada: só nomeia o que já aconteceu. O convite já foi gasto quando
/// isto roda — a linha continua lá, com `usado_em` preenchido, e é dela que sai
/// a observação que quem hospeda escreveu ao gerá-lo.
///
/// Um segredo que não é convite nenhum e ainda assim passou só pode ter sido a
/// senha, porque `admitir` não tem terceira porta. Deduzir em vez de recalcular
/// evita rodar Argon2 de novo, que é caro de propósito.
#[must_use]
pub fn como_chegou(
    persistence: &Persistence,
    aberto: bool,
    segredo: Option<&str>,
) -> (Segredo, String) {
    if aberto {
        return (Segredo::Aberto, String::new());
    }
    let Some(segredo) = segredo else {
        return (Segredo::Aberto, String::new());
    };

    let observacao: Option<String> = persistence
        .connection()
        .query_row(
            "SELECT observacao FROM convites WHERE token = ?1",
            params![segredo],
            |linha| linha.get(0),
        )
        .optional()
        .ok()
        .flatten();

    match observacao {
        Some(observacao) => (Segredo::Convite, observacao),
        None => (Segredo::Senha, String::new()),
    }
}

/// Decide se uma chave **já provada** entra, e registra a batida.
///
/// Chamada com a impressão de uma chave cuja assinatura já foi verificada. Uma
/// chave que ninguém provou não tem o que fazer nesta tabela — ver o cabeçalho
/// do módulo.
///
/// Registrar e decidir são a mesma operação de propósito: um pedido que só é
/// gravado depois de a tela abrir é um pedido que se perde quando ninguém está
/// olhando, que é o caso que este módulo existe para cobrir.
///
/// # Errors
///
/// Falha se o banco não responder. Uma recusa é `Ok(Resposta::Recusado)`, não
/// erro: recusar é resultado normal.
pub fn bater(
    persistence: &mut Persistence,
    impressao: &str,
    apelido: &str,
    segredo: Segredo,
    observacao: &str,
) -> Result<Resposta> {
    if !ligada(persistence)? {
        return Ok(Resposta::Entra);
    }

    let agora = now_seconds();
    let conexao = persistence.connection();

    let atual: Option<(String, Option<i64>)> = conexao
        .query_row(
            "SELECT veredito, decidido_em FROM portaria WHERE impressao = ?1",
            params![impressao],
            |linha| Ok((linha.get(0)?, linha.get(1)?)),
        )
        .optional()?;

    match atual {
        Some((veredito, Some(_))) => {
            // Já decidido. O apelido **não** é reescrito aqui: o que quem
            // hospeda aprovou foi o que estava no cartão, e deixar a pessoa
            // trocar o nome depois de aprovada faria a lista de admitidos
            // mentir sobre quem foi admitido. O nome em uso é do `people`, e
            // esta coluna é o registro do que foi decidido.
            if veredito == "admitido" {
                Ok(Resposta::Entra)
            } else {
                Ok(Resposta::Recusado)
            }
        }
        Some(_) => {
            // Pendente, e bateu de novo. Tentar de novo é o caminho que a frase
            // do `AdmissionPending` manda seguir, então não pode virar linha
            // nova nem apagar a hora da primeira batida — quem hospeda quer
            // saber há quanto tempo a pessoa está esperando, não quando ela
            // desistiu de esperar pela última vez.
            //
            // O apelido acompanha: aqui ainda não há decisão para contradizer,
            // e alguém que corrigiu um erro de digitação deve aparecer
            // corrigido no cartão que ninguém leu ainda.
            conexao.execute(
                "UPDATE portaria
                    SET batidas = batidas + 1, apelido = ?2, segredo = ?3, observacao = ?4
                  WHERE impressao = ?1",
                params![impressao, apelido, segredo.nome(), observacao],
            )?;
            Ok(Resposta::Pendente)
        }
        None => {
            conexao.execute(
                "INSERT INTO portaria
                     (impressao, veredito, apelido, segredo, observacao, bateu_em)
                 VALUES (?1, 'pendente', ?2, ?3, ?4, ?5)",
                params![impressao, apelido, segredo.nome(), observacao, agora],
            )?;
            Ok(Resposta::Pendente)
        }
    }
}

/// Admite de saída quem hospeda, no próprio servidor.
///
/// # Por que isto existe
///
/// A portaria trancou o dono para fora da própria casa, e o relato veio de um
/// app instalado: apertar **HOSPEDAR AQUI** subia o servidor, o app conectava
/// nele, e o porteiro tratava quem hospeda como desconhecido — o pedido ficava
/// esperando a decisão de alguém que não conseguia entrar para decidir.
/// Deadlock no caminho principal do produto.
///
/// O desenho do ADR 0030 não tinha noção de dono porque o porteiro decide sobre
/// **quem chega**, e quem hospeda não chega: já estava aqui.
///
/// Chamado no momento de hospedar, e não na primeira conexão: se dependesse da
/// primeira conexão seria a mesma corrida, porque a decisão precisa existir
/// antes de haver alguém a decidir.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn admitir_o_dono(persistence: &mut Persistence, impressao: &str) -> Result<()> {
    let agora = now_seconds();
    persistence.connection().execute(
        "INSERT INTO portaria
             (impressao, veredito, apelido, segredo, observacao, bateu_em, decidido_em)
         VALUES (?1, 'admitido', '', 'aberto', 'quem hospeda', ?2, ?2)
         ON CONFLICT(impressao) DO UPDATE
            SET veredito = 'admitido', decidido_em = ?2",
        params![impressao, agora],
    )?;
    Ok(())
}

/// Quem hospeda decide sobre um pedido.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn decidir(persistence: &mut Persistence, impressao: &str, admitir: bool) -> Result<()> {
    let veredito = if admitir { "admitido" } else { "recusado" };
    persistence.connection().execute(
        "UPDATE portaria SET veredito = ?2, decidido_em = ?3 WHERE impressao = ?1",
        params![impressao, veredito, now_seconds()],
    )?;
    Ok(())
}

/// Desfaz uma decisão: a pessoa volta a ser desconhecida.
///
/// **Não é banir, e não derruba quem está dentro.** Revogar diz «pergunte-me
/// outra vez»; banir diz «nunca», e para derrubar já existe expulsar. Fazer um
/// ato brando ter consequência violenta é como uma interface ensina a não
/// apertar nada.
///
/// É a linha que se apaga, e é de propósito que seja exatamente isso: o buraco
/// já registrado do `unban` — que existe em `Permissions` e não tem verbo de
/// protocolo — tem esta mesma forma, `DELETE FROM bans`. Este módulo não o
/// conserta; mostra que a forma serve.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn revogar(persistence: &mut Persistence, impressao: &str) -> Result<()> {
    persistence.connection().execute(
        "DELETE FROM portaria WHERE impressao = ?1",
        params![impressao],
    )?;
    Ok(())
}

/// Se esta impressão digital **já foi admitida** por quem hospeda.
///
/// Estreito de propósito, e a estreiteza é a segurança desta função: responde
/// `true` só quando a portaria está ligada **e** existe uma decisão gravada de
/// `admitido` para esta impressão. Não é [`bater`], que responde `Entra` a todo
/// mundo quando a portaria está desligada — usar aquela aqui deixaria qualquer
/// segredo errado entrar em qualquer servidor sem portaria.
///
/// # Para que ela existe
///
/// A política de admissão não tem memória: com o servidor fechado, ela exige
/// segredo de todo mundo, sempre. O convite que trouxe alguém é de uso único e
/// é gasto na entrada — então, na volta, a pessoa aprovada é barrada por
/// `ConviteGasto` **antes** de a portaria poder dizer que a conhece.
///
/// Relatado em campo como «aprovei a entrada de alguém e deu como credencial
/// recusada». Quem recusou não foi a portaria: foi a porta de fora, que não
/// sabia que a de dentro já tinha aberto.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn ja_admitido(persistence: &Persistence, impressao: &str) -> Result<bool> {
    if !ligada(persistence)? {
        return Ok(false);
    }
    let decidido: Option<String> = persistence
        .connection()
        .query_row(
            "SELECT veredito FROM portaria
              WHERE impressao = ?1 AND decidido_em IS NOT NULL",
            params![impressao],
            |linha| linha.get(0),
        )
        .optional()?;
    Ok(decidido.as_deref() == Some("admitido"))
}

/// Se esta impressão digital **tem um pedido esperando decisão**.
///
/// # Por que ela existe, e é a irmã de [`ja_admitido`]
///
/// Aquela perdoa um segredo errado para quem já foi admitido. Esta perdoa o
/// mesmo para quem **ainda está na fila** — e a razão é que a tela de espera do
/// cliente bate de novo a cada quinze segundos **sem segredo nenhum**: o convite
/// é de uso único e `conectar` não o lembra de propósito, porque reenviá-lo
/// numa reconexão seria gastar de novo o que já foi gasto.
///
/// Sem isto, a batida que a própria tela repete para na camada do segredo, e a
/// pessoa que está esperando permissão recebe **«credencial recusada»** — uma
/// resposta sobre um problema que ela não tem, no meio de uma espera que está
/// correndo normalmente. Foi relatado assim: «o portão barra e pede liberação, o
/// host libera, e no recall ele não reconhece e fica no loop».
///
/// # Por que não abre buraco nenhum
///
/// Três coisas, e as três precisam ser verdade ao mesmo tempo:
///
///   1. a impressão chega aqui **provada** — a assinatura sobre o nonce já foi
///      conferida, como em [`ja_admitido`];
///   2. só existe linha na `portaria` para quem **já passou pela camada do
///      segredo uma vez**, porque [`bater`] só roda depois dela;
///   3. e o que se ganha é continuar esperando, não entrar.
///
/// Ou seja: quem recebe este perdão é exatamente quem já provou o segredo e a
/// chave, e o que ele recebe de volta é a mesma porta fechada com o nome certo.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn pedido_pendente(persistence: &Persistence, impressao: &str) -> Result<bool> {
    if !ligada(persistence)? {
        return Ok(false);
    }
    let pendente: Option<i64> = persistence
        .connection()
        .query_row(
            "SELECT 1 FROM portaria
              WHERE impressao = ?1 AND decidido_em IS NULL",
            params![impressao],
            |linha| linha.get(0),
        )
        .optional()?;
    Ok(pendente.is_some())
}

/// A fila e o histórico, pendentes primeiro e mais antigo antes.
///
/// Pendentes primeiro porque é o que pede ação; mais antigo antes porque quem
/// esperou mais tem precedência, e porque uma fila que reordena sozinha é uma
/// fila em que se aprova a pessoa errada.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn pedidos(persistence: &Persistence) -> Result<Vec<Pedido>> {
    let conexao = persistence.connection();
    let mut consulta = conexao.prepare(
        "SELECT impressao, apelido, segredo, observacao, bateu_em, batidas, decidido_em, veredito
           FROM portaria
          ORDER BY (decidido_em IS NOT NULL), bateu_em",
    )?;
    let linhas = consulta.query_map([], |linha| {
        let veredito: String = linha.get(7)?;
        Ok(Pedido {
            impressao: linha.get(0)?,
            apelido: linha.get(1)?,
            segredo: linha.get(2)?,
            observacao: linha.get(3)?,
            bateu_em: linha.get(4)?,
            batidas: linha.get(5)?,
            decidido_em: linha.get(6)?,
            admitido: veredito == "admitido",
        })
    })?;

    let mut fila = Vec::new();
    for linha in linhas {
        fila.push(linha?);
    }
    Ok(fila)
}

/// Quantos pedidos esperam decisão.
///
/// Para o cartão de hospedagem dizer que há gente batendo sem desenhar a fila
/// inteira a cada quarto de segundo.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn pendentes(persistence: &Persistence) -> Result<i64> {
    Ok(persistence.connection().query_row(
        "SELECT COUNT(*) FROM portaria WHERE decidido_em IS NULL",
        [],
        |linha| linha.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Location;

    fn persistence() -> Persistence {
        Persistence::open(&Location::Memory).expect("banco em memória")
    }

    const MARCELA: &str = "aaaa1111";
    const PIRES: &str = "bbbb2222";

    #[test]
    fn quem_hospeda_entra_na_propria_casa_sem_esperar_ninguem() {
        // O defeito que fechou o produto. Apertar HOSPEDAR AQUI subia o servidor,
        // o app conectava nele, e o porteiro tratava quem hospeda como
        // desconhecido — deixando o pedido esperando a decisão de alguém que
        // não conseguia entrar para decidir.
        let mut banco = persistence();
        ligar(&mut banco, true).unwrap();

        admitir_o_dono(&mut banco, MARCELA).unwrap();

        assert_eq!(
            bater(&mut banco, MARCELA, "quem hospeda", Segredo::Aberto, "").unwrap(),
            Resposta::Entra,
            "quem hospeda bateu na própria porta e ficou esperando alguém abrir"
        );

        // E não é «a portaria deixou de valer»: qualquer outra pessoa continua
        // esperando. Um conserto que abrisse a porta para todo mundo passaria
        // na asserção de cima e falharia aqui.
        assert_eq!(
            bater(&mut banco, PIRES, "estranha", Segredo::Aberto, "").unwrap(),
            Resposta::Pendente,
            "admitir o dono abriu a porta para todo mundo"
        );
    }

    #[test]
    fn hospedar_de_novo_nao_desfaz_uma_recusa_que_ja_foi_dada() {
        // Hospedar roda toda vez que a janela sobe um servidor, sobre o mesmo
        // banco. Tem de ser idempotente para o dono e inerte para todo o resto.
        let mut banco = persistence();
        ligar(&mut banco, true).unwrap();

        bater(&mut banco, PIRES, "indesejada", Segredo::Aberto, "").unwrap();
        decidir(&mut banco, PIRES, false).unwrap();

        admitir_o_dono(&mut banco, MARCELA).unwrap();
        admitir_o_dono(&mut banco, MARCELA).unwrap();

        assert_eq!(
            bater(&mut banco, MARCELA, "quem hospeda", Segredo::Aberto, "").unwrap(),
            Resposta::Entra
        );
        assert_eq!(
            bater(&mut banco, PIRES, "indesejada", Segredo::Aberto, "").unwrap(),
            Resposta::Recusado,
            "hospedar de novo desfez uma recusa que quem hospeda tinha dado"
        );
    }

    #[test]
    fn uma_portaria_desligada_deixa_tudo_como_estava() {
        // O comportamento de antes desta migração, e o do `seeled`, que o ADR
        // 0021 mantém aberto de propósito.
        let mut c = persistence();
        assert!(!ligada(&c).expect("ler"));
        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Entra
        );
        // E não grava nada: uma portaria desligada não é uma portaria que
        // registra em silêncio.
        assert_eq!(pendentes(&c).expect("contar"), 0);
    }

    #[test]
    fn o_primeiro_contato_de_uma_pessoa_fica_pendente_e_o_segundo_tambem() {
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");

        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Pendente
        );
        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Pendente
        );
        // Uma linha, não duas: tentar de novo é o que a frase manda fazer.
        assert_eq!(pendentes(&c).expect("contar"), 1);
        assert_eq!(pedidos(&c).expect("ler")[0].batidas, 2);
    }

    #[test]
    fn aprovado_uma_vez_entra_nas_proximas_sem_perguntar() {
        // A promessa inteira do TOFU: pergunta-se uma vez.
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");
        bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater");

        decidir(&mut c, MARCELA, true).expect("decidir");

        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Entra
        );
        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Entra
        );
        assert_eq!(pendentes(&c).expect("contar"), 0);
    }

    #[test]
    fn recusado_continua_recusado_e_nao_volta_para_a_fila() {
        // O modo de falhar que importa: se bater de novo devolvesse a decisão à
        // fila, recusar seria adiar, e quem foi recusado teria só que insistir.
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");
        bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater");
        decidir(&mut c, MARCELA, false).expect("decidir");

        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Recusado
        );
        assert_eq!(pendentes(&c).expect("contar"), 0);
    }

    #[test]
    fn decidir_sobre_uma_pessoa_nao_decide_sobre_outra() {
        // Aprovar é sobre uma chave. Se vazasse para as vizinhas, aprovar o
        // primeiro que bate abriria a porta para todos.
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");
        bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater");
        bater(&mut c, PIRES, "carla", Segredo::Aberto, "").expect("bater");

        decidir(&mut c, MARCELA, true).expect("decidir");

        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Entra
        );
        assert_eq!(
            bater(&mut c, PIRES, "carla", Segredo::Aberto, "").expect("bater"),
            Resposta::Pendente
        );
    }

    #[test]
    fn revogar_faz_a_pessoa_voltar_a_ser_desconhecida_em_vez_de_barrada() {
        // A diferença entre revogar e banir, no comportamento e não só na
        // prosa: depois de revogar, bater pergunta de novo — não recusa.
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");
        bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater");
        decidir(&mut c, MARCELA, true).expect("decidir");

        revogar(&mut c, MARCELA).expect("revogar");

        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Pendente
        );
    }

    #[test]
    fn um_pedido_guarda_o_convite_com_que_a_pessoa_chegou() {
        // «Chegou com o convite *para o Rafael*» é a melhor prova que existe do
        // outro lado, e `criar_convite` já guardava a observação sem que nada a
        // lesse.
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");
        bater(&mut c, MARCELA, "rei", Segredo::Convite, "para a Rei").expect("bater");

        let pedido = &pedidos(&c).expect("ler")[0];
        assert_eq!(pedido.segredo, "convite");
        assert_eq!(pedido.observacao, "para a Rei");
        // E não aprova sozinho. O convite é prova exibida, não decisão.
        assert!(!pedido.admitido);
        assert_eq!(pedido.decidido_em, None);
    }

    #[test]
    fn o_apelido_de_quem_ja_foi_decidido_nao_e_reescrito_por_uma_batida() {
        // Senão a lista de admitidos mentiria sobre quem foi admitido: bastaria
        // ser aprovado como `rei` e voltar dizendo-se `comandante`.
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");
        bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater");
        decidir(&mut c, MARCELA, true).expect("decidir");

        bater(&mut c, MARCELA, "comandante", Segredo::Aberto, "").expect("bater");

        assert_eq!(pedidos(&c).expect("ler")[0].apelido, "rei");
    }

    #[test]
    fn a_fila_poe_quem_espera_antes_de_quem_ja_foi_decidido() {
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");
        bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater");
        decidir(&mut c, MARCELA, true).expect("decidir");
        bater(&mut c, PIRES, "carla", Segredo::Aberto, "").expect("bater");

        let fila = pedidos(&c).expect("ler");
        assert_eq!(fila[0].impressao, PIRES, "o pendente vem primeiro");
        assert_eq!(fila[1].impressao, MARCELA);
    }

    #[test]
    fn a_semente_nao_rearma_o_interruptor_de_quem_o_desligou() {
        // `semear_ligada` roda toda vez que o app sobe um servidor. Um interruptor
        // que se rearma sozinho é um interruptor quebrado.
        let mut c = persistence();
        semear_ligada(&mut c).expect("semear");
        assert!(ligada(&c).expect("ler"));

        ligar(&mut c, false).expect("desligar");
        semear_ligada(&mut c).expect("semear de novo");

        assert!(!ligada(&c).expect("ler"));
    }

    #[test]
    fn desligar_a_portaria_nao_apaga_quem_ja_foi_recusado() {
        // Desligar é «pare de perguntar», não «esqueça o que eu decidi». Religar
        // tem que devolver as decisões de antes, ou desligar por um minuto
        // viraria o jeito de apagar uma recusa.
        let mut c = persistence();
        ligar(&mut c, true).expect("ligar");
        bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater");
        decidir(&mut c, MARCELA, false).expect("decidir");

        ligar(&mut c, false).expect("desligar");
        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Entra,
            "desligada, ela deixa passar"
        );

        ligar(&mut c, true).expect("religar");
        assert_eq!(
            bater(&mut c, MARCELA, "rei", Segredo::Aberto, "").expect("bater"),
            Resposta::Recusado
        );
    }
}

#[cfg(test)]
mod a_volta_de_quem_ja_foi_aprovado {
    #![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

    use super::*;
    use crate::admissao::{criar_convite, gastar, Politica};
    use crate::persistence::Location;

    const MARCELA: &str = "aaaa1111";

    /// Relatado em campo: «aprovei a entrada de alguém e deu como credencial
    /// recusada».
    ///
    /// A portaria lembra da pessoa — `aprovado_uma_vez_entra_nas_proximas_sem_
    /// perguntar` afirma isso —, mas ela nunca chega a ser consultada. A
    /// política de admissão roda **antes**, no `session::serve`, e ela não tem
    /// memória nenhuma: com o servidor fechado, exige segredo de todo mundo,
    /// sempre. O convite de uso único que trouxe a pessoa foi gasto na entrada
    /// dela, e na volta ele é `ConviteGasto`.
    ///
    /// O resultado é que **quem foi aprovado não consegue voltar**. Não é a
    /// portaria recusando: é a porta de fora, que não sabe que a de dentro já
    /// abriu.
    #[test]
    fn quem_foi_aprovado_volta_sem_precisar_de_um_convite_novo() {
        let mut persistence = Persistence::open(&Location::Memory).expect("banco em memória");
        // A portaria vem desligada num banco novo, e sem ela `bater` responde
        // `Entra` a todo mundo — o cenário deste teste não existiria.
        ligar(&mut persistence, true).expect("ligar a portaria");
        let token = criar_convite(&mut persistence, "rafa").expect("criar convite");

        // A chegada: o convite vale, a portaria põe em espera.
        let politica = Politica::carregar(&persistence).expect("política");
        let passe = politica
            .avaliar(&persistence, Some(&token))
            .expect("avaliar")
            .expect("o convite vale na chegada");
        let (segredo, observacao) = como_chegou(&persistence, politica.aberto(), Some(&token));
        assert_eq!(
            bater(&mut persistence, MARCELA, "rafa", segredo, &observacao).expect("bater"),
            Resposta::Pendente
        );

        // Quem hospeda aprova, e a pessoa entra. O convite é gasto agora.
        decidir(&mut persistence, MARCELA, true).expect("aprovar");
        let politica = Politica::carregar(&persistence).expect("política");
        let (segredo, observacao) = como_chegou(&persistence, politica.aberto(), Some(&token));
        assert_eq!(
            bater(&mut persistence, MARCELA, "rafa", segredo, &observacao).expect("bater"),
            Resposta::Entra
        );
        gastar(&mut persistence, &passe)
            .expect("gastar")
            .expect("o convite é gasto na entrada");

        // E ela volta no dia seguinte, com o mesmo link — que é o único que
        // ela tem. A política **recusa**, e isso está certo: o convite é de uso
        // único e foi gasto. Não é aqui que o conserto mora.
        let politica = Politica::carregar(&persistence).expect("política");
        assert!(
            politica
                .avaliar(&persistence, Some(&token))
                .expect("avaliar")
                .is_err(),
            "o convite de uso único deixou de ser de uso único"
        );

        // O conserto é a composição: `session::serve` guarda essa recusa em vez
        // de devolvê-la, e a descarta depois da assinatura se esta chave já
        // tiver decisão de admitida. É esta a pergunta que faltava.
        assert!(
            ja_admitido(&persistence, MARCELA).expect("perguntar"),
            "quem já foi aprovado não é reconhecido na volta, e a política a \
             barra por convite gasto antes de a portaria poder dizer que a \
             conhece"
        );

        // E a estreiteza que faz isso ser seguro: com a portaria desligada,
        // ninguém é «já admitido». Sem esta linha, o perdão da recusa deixaria
        // qualquer segredo errado entrar em todo servidor que não usa portaria.
        ligar(&mut persistence, false).expect("desligar a portaria");
        assert!(
            !ja_admitido(&persistence, MARCELA).expect("perguntar"),
            "com a portaria desligada alguém continua contando como admitido, \
             e o perdão da recusa vira uma porta escancarada"
        );
    }
}
