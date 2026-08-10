//! Quem pode entrar num Dogma.
//!
//! Fecha o **[EM ABERTO — escolher em M2]** de `specs/08-seguranca.md`, que
//! recomendava: "chave pública como mecanismo primário, com convite por token
//! de uso único para entrada em um Dogma. Senha como fallback opcional
//! configurável pelo operador."
//!
//! Até M5 não havia porteiro nenhum: quem alcançasse a porta UDP completava o
//! handshake e ganhava uma conta. Para uma rede local entre duas máquinas isso
//! é o comportamento certo, e é por isso que passou tanto tempo despercebido.
//! No dia em que alguém abre a porta no roteador, deixa de ser.
//!
//! # Os dois mecanismos, e por que os dois
//!
//! **Convite de uso único** é o primário. O operador gera um, manda para uma
//! pessoa, e ele morre ao ser usado. É o que torna uma URL compartilhável
//! segura: um token já gasto que vaze depois não vale nada — coisa que jamais
//! se pode dizer de uma senha.
//!
//! **Senha do Dogma** é a alternativa, para quem prefere um segredo só para o
//! grupo todo. Argon2id, como `specs/08` exige. Mais frágil por natureza: um
//! segredo compartilhado vaza pelo membro mais descuidado e não dá para
//! revogar sem trocar para todo mundo.
//!
//! Um Dogma sem nenhum dos dois configurados é **aberto**, e continua sendo o
//! padrão: é o que faz o teste em rede local funcionar sem cerimônia. O
//! `seeled` avisa em voz alta ao subir assim escutando fora do loopback.

use anyhow::{Context, Result};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rusqlite::{params, Connection};

use seele_proto::ids::CageId;

use crate::casper::{now_seconds, Casper};

/// Quanto tempo um convite vale, em segundos. Sete dias.
///
/// Um convite eterno é uma senha com nome diferente. O prazo é o que faz dele
/// um convite.
pub const VALIDADE_CONVITE_S: i64 = 7 * 24 * 60 * 60;

/// Por que a entrada foi recusada.
///
/// Não atravessa a rede: `specs/08-seguranca.md` exige que falhas de entrada
/// sejam **uniformes** para quem está do lado de fora, senão o próprio erro
/// vira um oráculo — "senha errada" e "convite já usado" contam coisas
/// diferentes a quem está tentando adivinhar. O servidor responde sempre
/// `CredentialRejected`; esta enumeração existe para o log do operador, que
/// tem direito de saber o que houve na máquina dele.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recusa {
    /// O Dogma exige segredo e nenhum foi apresentado.
    SegredoAusente,
    /// Não é a senha nem um convite conhecido.
    SegredoInvalido,
    /// Era um convite, e já foi usado.
    ConviteGasto,
    /// Era um convite, e passou do prazo.
    ConviteExpirado,
}

/// Como o Dogma decide quem entra.
#[derive(Debug, Clone, Default)]
pub struct Politica {
    /// Hash Argon2id da senha do Dogma, se o operador configurou uma.
    senha_hash: Option<String>,
    /// Se convites são aceitos. Ligado sempre que existe pelo menos um.
    aceita_convites: bool,
}

impl Politica {
    /// Um Dogma sem porteiro.
    #[must_use]
    pub fn aberta() -> Self {
        Self::default()
    }

    /// Lê a política que está no banco.
    ///
    /// # Errors
    ///
    /// Falha se o banco não responder.
    pub fn carregar(casper: &Casper) -> Result<Self> {
        let conexao = casper.connection();
        let senha_hash: Option<String> = conexao
            .query_row(
                "SELECT valor FROM configuracao WHERE chave = 'senha_dogma'",
                [],
                |linha| linha.get(0),
            )
            .ok();
        let convites: i64 = conexao
            .query_row("SELECT COUNT(*) FROM convites", [], |linha| linha.get(0))
            .unwrap_or(0);

        Ok(Self {
            senha_hash,
            aceita_convites: convites > 0,
        })
    }

    /// Se este Dogma deixa qualquer um entrar.
    #[must_use]
    pub fn aberto(&self) -> bool {
        self.senha_hash.is_none() && !self.aceita_convites
    }

    /// Decide se um segredo apresentado abre a porta.
    ///
    /// Consome o convite quando for o caso, e é por isso que precisa da
    /// conexão: a decisão e o consumo têm de ser a mesma operação, ou dois
    /// clientes usam o mesmo convite ao mesmo tempo.
    ///
    /// # Errors
    ///
    /// Falha se o banco não responder. Recusa é `Ok(Err(_))`, não erro.
    pub fn admitir(
        &self,
        casper: &mut Casper,
        segredo: Option<&str>,
    ) -> Result<Result<(), Recusa>> {
        if self.aberto() {
            return Ok(Ok(()));
        }

        let Some(segredo) = segredo.filter(|s| !s.is_empty()) else {
            return Ok(Err(Recusa::SegredoAusente));
        };

        if let Some(hash) = &self.senha_hash {
            if verificar_senha(segredo, hash) {
                return Ok(Ok(()));
            }
        }

        if self.aceita_convites {
            return consumir_convite(casper.connection(), segredo);
        }

        Ok(Err(Recusa::SegredoInvalido))
    }
}

/// Grava a senha do Dogma, ou a remove.
///
/// # Errors
///
/// Falha se o hash não puder ser calculado ou o banco não responder.
pub fn definir_senha(casper: &mut Casper, senha: Option<&str>) -> Result<()> {
    let conexao = casper.connection();
    match senha {
        None => {
            conexao.execute("DELETE FROM configuracao WHERE chave = 'senha_dogma'", [])?;
        }
        Some(senha) => {
            let hash = calcular_hash(senha)?;
            conexao.execute(
                "INSERT INTO configuracao (chave, valor) VALUES ('senha_dogma', ?1)
                 ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
                params![hash],
            )?;
        }
    }
    Ok(())
}

/// Cria um convite de uso único e devolve o token.
///
/// O token é aleatório de 160 bits em base32 sem caracteres ambíguos — dá para
/// ditar por telefone sem confundir zero com O.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn criar_convite(casper: &mut Casper, observacao: &str) -> Result<String> {
    let token = gerar_token();
    let agora = now_seconds();
    casper.connection().execute(
        "INSERT INTO convites (token, criado_em, expira_em, observacao) VALUES (?1, ?2, ?3, ?4)",
        params![token, agora, agora + VALIDADE_CONVITE_S, observacao],
    )?;
    Ok(token)
}

/// Se um Cage aceita esta entrada.
///
/// A senha do Cage é coisa diferente da admissão no Dogma: aquela decide quem
/// entra na casa, esta decide quem entra num cômodo. Um Cage sem senha aceita
/// qualquer membro do Dogma, que é o caso normal.
///
/// Recusa quando o banco não responde. É a única resposta segura: uma consulta
/// que falha não é prova de que a porta pode abrir.
#[must_use]
pub fn cage_liberado(casper: &Casper, cage: CageId, senha: Option<&str>) -> bool {
    let hash: Option<Option<String>> = casper
        .connection()
        .query_row(
            "SELECT password_hash FROM cages WHERE id = ?1",
            params![i64::from(cage.get())],
            |linha| linha.get(0),
        )
        .ok();

    match hash {
        // Cage inexistente. Recusar aqui evita que um erro de digitação vire
        // uma entrada silenciosa em lugar nenhum.
        None => false,
        Some(None) => true,
        Some(Some(hash)) => senha.is_some_and(|senha| verificar_senha(senha, &hash)),
    }
}

/// Grava a senha de um Cage, ou a remove.
///
/// # Errors
///
/// Falha se o hash não puder ser calculado ou o banco não responder.
pub fn definir_senha_cage(casper: &mut Casper, cage: CageId, senha: Option<&str>) -> Result<()> {
    let hash = senha.map(calcular_hash).transpose()?;
    casper.connection().execute(
        "UPDATE cages SET password_hash = ?1 WHERE id = ?2",
        params![hash, i64::from(cage.get())],
    )?;
    Ok(())
}

/// Consome um convite, se ele existir e ainda valer.
fn consumir_convite(conexao: &Connection, token: &str) -> Result<Result<(), Recusa>> {
    let encontrado: Option<(i64, Option<i64>)> = conexao
        .query_row(
            "SELECT expira_em, usado_em FROM convites WHERE token = ?1",
            params![token],
            |linha| Ok((linha.get(0)?, linha.get(1)?)),
        )
        .ok();

    let Some((expira_em, usado_em)) = encontrado else {
        return Ok(Err(Recusa::SegredoInvalido));
    };
    if usado_em.is_some() {
        return Ok(Err(Recusa::ConviteGasto));
    }
    let agora = now_seconds();
    if agora > expira_em {
        return Ok(Err(Recusa::ConviteExpirado));
    }

    // `usado_em IS NULL` na cláusula: se dois clientes chegarem com o mesmo
    // convite no mesmo instante, só um vê linha alterada. A verificação acima
    // sozinha seria uma corrida.
    let alteradas = conexao.execute(
        "UPDATE convites SET usado_em = ?1 WHERE token = ?2 AND usado_em IS NULL",
        params![agora, token],
    )?;

    if alteradas == 1 {
        Ok(Ok(()))
    } else {
        Ok(Err(Recusa::ConviteGasto))
    }
}

/// Argon2id com os parâmetros padrão do crate, que são os recomendados atuais.
///
/// `specs/08-seguranca.md` pede Argon2id nominalmente.
fn calcular_hash(senha: &str) -> Result<String> {
    // Salt do nosso próprio gerador em vez do `OsRng` do crate: a feature que
    // o expõe não está ligada, e o `rand` já é dependência daqui. Dezesseis
    // bytes é o recomendado pelo Argon2.
    let bruto: [u8; 16] = rand::random();
    let salt = SaltString::encode_b64(&bruto)
        .map_err(|erro| anyhow::anyhow!("não consegui gerar o salt: {erro}"))?;
    Argon2::default()
        .hash_password(senha.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|erro| anyhow::anyhow!("não consegui calcular o hash: {erro}"))
        .context("Argon2id")
}

/// Compara em tempo constante. A comparação vem do próprio Argon2.
fn verificar_senha(senha: &str, hash: &str) -> bool {
    let Ok(analisado) = PasswordHash::new(hash) else {
        // Hash corrompido no banco. Recusar é a única resposta segura: aceitar
        // seria transformar corrupção em porta aberta.
        tracing::error!("o hash da senha do Dogma está corrompido; ninguém entra por senha");
        return false;
    };
    Argon2::default()
        .verify_password(senha.as_bytes(), &analisado)
        .is_ok()
}

/// 160 bits de aleatoriedade do sistema, em base32 legível.
///
/// Sem `0`, `1`, `I`, `O` e `L`: o alfabeto de Crockford, porque este token vai
/// ser lido em voz alta e digitado à mão pelo menos uma vez.
fn gerar_token() -> String {
    const ALFABETO: &[u8] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
    let bruto: [u8; 20] = rand::random();
    bruto
        .iter()
        .filter_map(|byte| {
            ALFABETO
                .get(usize::from(*byte) % ALFABETO.len())
                .map(|c| char::from(*c))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casper::Location;

    fn casper() -> Casper {
        let casper = Casper::open(&Location::Memory).expect("banco em memória");
        // O Dogma real semeia o Cage padrão ao subir; um banco recém-migrado
        // está vazio, e um teste de fechadura precisa de uma porta.
        casper
            .connection()
            .execute(
                "INSERT INTO cages (id, name, member_limit, position) VALUES (1, 'CAGE-01', 20, 0)",
                [],
            )
            .expect("semear o Cage");
        casper
    }

    #[test]
    fn um_dogma_sem_configuracao_deixa_qualquer_um_entrar() {
        // É o padrão de propósito: é o que faz o teste em rede local funcionar
        // sem cerimônia. O aviso ao subir é que impede isso de ser esquecido.
        let mut casper = casper();
        let politica = Politica::carregar(&casper).expect("política");

        assert!(politica.aberto());
        assert_eq!(
            politica.admitir(&mut casper, None).expect("admitir"),
            Ok(())
        );
    }

    #[test]
    fn com_senha_configurada_a_porta_fecha() {
        let mut casper = casper();
        definir_senha(&mut casper, Some("terceiro impacto")).expect("definir");
        let politica = Politica::carregar(&casper).expect("política");

        assert!(!politica.aberto());
        assert_eq!(
            politica.admitir(&mut casper, None).expect("admitir"),
            Err(Recusa::SegredoAusente)
        );
        assert_eq!(
            politica
                .admitir(&mut casper, Some("errada"))
                .expect("admitir"),
            Err(Recusa::SegredoInvalido)
        );
        assert_eq!(
            politica
                .admitir(&mut casper, Some("terceiro impacto"))
                .expect("admitir"),
            Ok(())
        );
    }

    #[test]
    fn a_senha_nao_fica_em_texto_puro_no_banco() {
        // O operador tem o arquivo de qualquer jeito; o que se protege é o dia
        // em que o banco vaza por outro caminho — backup, disco, engano.
        let mut casper = casper();
        definir_senha(&mut casper, Some("terceiro impacto")).expect("definir");

        let guardado: String = casper
            .connection()
            .query_row(
                "SELECT valor FROM configuracao WHERE chave = 'senha_dogma'",
                [],
                |linha| linha.get(0),
            )
            .expect("ler");

        assert!(!guardado.contains("terceiro impacto"));
        assert!(guardado.starts_with("$argon2id$"), "{guardado}");
    }

    #[test]
    fn um_convite_vale_uma_vez_e_so() {
        // A propriedade que torna seguro pôr um convite numa URL: depois de
        // usado, vazar não vale nada.
        let mut casper = casper();
        let token = criar_convite(&mut casper, "ayanami").expect("criar");
        let politica = Politica::carregar(&casper).expect("política");

        assert_eq!(
            politica
                .admitir(&mut casper, Some(&token))
                .expect("admitir"),
            Ok(())
        );
        assert_eq!(
            politica
                .admitir(&mut casper, Some(&token))
                .expect("admitir"),
            Err(Recusa::ConviteGasto)
        );
    }

    #[test]
    fn um_convite_vencido_nao_abre() {
        let mut casper = casper();
        let token = criar_convite(&mut casper, "atrasado").expect("criar");
        casper
            .connection()
            .execute(
                "UPDATE convites SET expira_em = ?1 WHERE token = ?2",
                params![now_seconds() - 1, token],
            )
            .expect("envelhecer");

        let politica = Politica::carregar(&casper).expect("política");
        assert_eq!(
            politica
                .admitir(&mut casper, Some(&token))
                .expect("admitir"),
            Err(Recusa::ConviteExpirado)
        );
    }

    #[test]
    fn senha_e_convite_convivem() {
        // O operador pode ter uma senha para o grupo fixo e convites para
        // visitas, sem escolher entre os dois.
        let mut casper = casper();
        definir_senha(&mut casper, Some("terceiro impacto")).expect("definir");
        let token = criar_convite(&mut casper, "visita").expect("criar");
        let politica = Politica::carregar(&casper).expect("política");

        assert_eq!(
            politica
                .admitir(&mut casper, Some("terceiro impacto"))
                .expect("admitir"),
            Ok(())
        );
        assert_eq!(
            politica
                .admitir(&mut casper, Some(&token))
                .expect("admitir"),
            Ok(())
        );
    }

    #[test]
    fn tirar_a_senha_reabre_o_dogma_se_nao_houver_convites() {
        let mut casper = casper();
        definir_senha(&mut casper, Some("temporária")).expect("definir");
        assert!(!Politica::carregar(&casper).expect("política").aberto());

        definir_senha(&mut casper, None).expect("remover");
        assert!(Politica::carregar(&casper).expect("política").aberto());
    }

    #[test]
    fn dois_tokens_nunca_saem_iguais() {
        let mut casper = casper();
        let tokens: std::collections::HashSet<String> = (0..200)
            .map(|_| criar_convite(&mut casper, "").expect("criar"))
            .collect();
        assert_eq!(tokens.len(), 200);
    }

    #[test]
    fn o_token_da_para_ditar_por_telefone() {
        // Sem 0, 1, I, O e L. Alguém vai ler isto em voz alta.
        let mut casper = casper();
        let token = criar_convite(&mut casper, "").expect("criar");

        assert_eq!(token.len(), 20);
        for confuso in ['0', '1', 'I', 'O', 'L'] {
            assert!(!token.contains(confuso), "{token} contém {confuso}");
        }
    }

    #[test]
    fn um_cage_sem_senha_aceita_qualquer_membro() {
        let casper = casper();
        assert!(cage_liberado(&casper, CageId(1), None));
    }

    #[test]
    fn um_cage_com_senha_recusa_quem_nao_a_tem() {
        // A fechadura que existia no protocolo, era anunciada ao cliente em
        // `password_required` e nunca era conferida.
        let mut casper = casper();
        definir_senha_cage(&mut casper, CageId(1), Some("geofront")).expect("definir");

        assert!(!cage_liberado(&casper, CageId(1), None));
        assert!(!cage_liberado(&casper, CageId(1), Some("errada")));
        assert!(cage_liberado(&casper, CageId(1), Some("geofront")));
    }

    #[test]
    fn um_cage_que_nao_existe_nao_libera() {
        // Um erro de digitação não deve virar entrada silenciosa em lugar
        // nenhum.
        let casper = casper();
        assert!(!cage_liberado(&casper, CageId(9999), None));
    }

    #[test]
    fn a_senha_do_cage_tambem_e_hasheada() {
        let mut casper = casper();
        definir_senha_cage(&mut casper, CageId(1), Some("geofront")).expect("definir");

        let guardado: String = casper
            .connection()
            .query_row("SELECT password_hash FROM cages WHERE id = 1", [], |l| {
                l.get(0)
            })
            .expect("ler");
        assert!(guardado.starts_with("$argon2id$"), "{guardado}");
    }

    #[test]
    fn um_hash_corrompido_recusa_em_vez_de_aceitar() {
        // A falha tem de cair para o lado fechado.
        let mut casper = casper();
        casper
            .connection()
            .execute(
                "INSERT INTO configuracao (chave, valor) VALUES ('senha_dogma', 'lixo')",
                [],
            )
            .expect("gravar lixo");

        let politica = Politica::carregar(&casper).expect("política");
        assert_eq!(
            politica
                .admitir(&mut casper, Some("lixo"))
                .expect("admitir"),
            Err(Recusa::SegredoInvalido)
        );
    }
}
