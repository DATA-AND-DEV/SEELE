//! A agenda: os códigos com quem esta máquina já falou.
//!
//! Spec de 23/09, «"SEU CÓDIGO" e a agenda». Chaveada pelo código, e não pelo
//! endereço: o código é o «número de telefone», e um endereço muda quando a
//! pessoa troca de rede.
//!
//! # Conveniência, e não segurança
//!
//! O mesmo raciocínio de [`crate::conhecidos`]. A chave inteira de quem já
//! ligou fica no arquivo de pins, sob `@CÓDIGO`, e é ela que decide se quem
//! atendeu é quem diz ser. Esta lista só lembra como **eu** chamo a pessoa,
//! que apelido ela usou da última vez, quando foi e se está bloqueada. Apagar
//! este arquivo desbloqueia todo mundo e esquece os nomes, e não enfraquece
//! conexão nenhuma.
//!
//! # Formato
//!
//! Uma linha por código, campos separados por tabulação:
//!
//! ```text
//! K9P2-CSM7-NBFR-AYQ3 <TAB> Rafa <TAB> rafa <TAB> 1738000000 <TAB> recebi <TAB> 0
//! ```
//!
//! código, o nome que eu dei, o apelido visto, a última ligação, a direção e o
//! bloqueio. Texto, porque alguém vai querer editar à mão. Colunas novas vão
//! sempre no fim.

use std::path::PathBuf;

use anyhow::{Context, Result};
use seele_proto::codigo::Codigo;

/// Para que lado foi a última ligação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direcao {
    /// Eu liguei.
    Liguei,
    /// Ligaram, e eu atendi.
    Recebi,
    /// Ligaram, e ninguém atendeu: a «chamada perdida» da spec.
    Perdida,
}

impl Direcao {
    fn texto(self) -> &'static str {
        match self {
            Self::Liguei => "liguei",
            Self::Recebi => "recebi",
            Self::Perdida => "perdida",
        }
    }

    fn ler(texto: &str) -> Option<Self> {
        match texto {
            "liguei" => Some(Self::Liguei),
            "recebi" => Some(Self::Recebi),
            "perdida" => Some(Self::Perdida),
            _ => None,
        }
    }
}

/// Uma pessoa da agenda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contato {
    /// O código: a única coisa desta linha que é prova.
    pub codigo: Codigo,
    /// Como eu chamo esta pessoa. Nunca trocado pelo que veio do outro lado.
    pub nome: Option<String>,
    /// O apelido que ela usou da última vez: o que **ela** diz.
    pub apelido: Option<String>,
    /// A última ligação, em segundos desde a época.
    pub visto_em: Option<i64>,
    /// Para que lado foi a última ligação.
    pub direcao: Option<Direcao>,
    /// Se as ligações dela são recusadas sem tocar.
    pub bloqueado: bool,
}

/// A agenda, em disco.
#[derive(Debug, Clone)]
pub struct Agenda {
    caminho: PathBuf,
    contatos: Vec<Contato>,
}

impl Agenda {
    /// Lê a agenda, ou começa uma vazia.
    ///
    /// Uma linha ilegível é pulada: isto é conveniência, e recusar abrir o app
    /// por uma agenda corrompida seria a troca errada. O mesmo vale para um
    /// código com o verificador errado, que é uma linha editada à mão.
    ///
    /// # Errors
    ///
    /// Falha só se o diretório não puder ser criado.
    pub fn abrir(caminho: PathBuf) -> Result<Self> {
        if let Some(pai) = caminho.parent() {
            std::fs::create_dir_all(pai)
                .with_context(|| format!("não consegui criar {}", pai.display()))?;
        }
        let contatos = std::fs::read_to_string(&caminho)
            .map(|texto| texto.lines().filter_map(ler_linha).collect())
            .unwrap_or_default();
        Ok(Self { caminho, contatos })
    }

    /// Todos, na ordem do arquivo. A casca ordena pelo que quiser mostrar.
    #[must_use]
    pub fn listar(&self) -> &[Contato] {
        &self.contatos
    }

    /// A pessoa deste código, se ela está na agenda.
    #[must_use]
    pub fn buscar(&self, codigo: &Codigo) -> Option<&Contato> {
        self.contatos
            .iter()
            .find(|contato| contato.codigo == *codigo)
    }

    /// Anota uma ligação: o apelido que veio, quando e para que lado.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser gravado.
    pub fn anotar_ligacao(
        &mut self,
        codigo: Codigo,
        apelido: &str,
        direcao: Direcao,
        quando: i64,
    ) -> Result<()> {
        let apelido = Some(limpo(apelido)).filter(|a| !a.is_empty());
        self.mudar(codigo, |contato| {
            contato.apelido = apelido;
            contato.visto_em = Some(quando);
            contato.direcao = Some(direcao);
        })
    }

    /// Dá um nome a uma pessoa. Um nome vazio tira o nome.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser gravado.
    pub fn dar_nome(&mut self, codigo: Codigo, nome: &str) -> Result<()> {
        let nome = Some(limpo(nome)).filter(|n| !n.is_empty());
        self.mudar(codigo, |contato| contato.nome = nome)
    }

    /// Bloqueia ou desbloqueia. Bloquear quem nunca ligou cria a linha.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser gravado.
    pub fn bloquear(&mut self, codigo: Codigo, bloqueado: bool) -> Result<()> {
        self.mudar(codigo, |contato| contato.bloqueado = bloqueado)
    }

    /// Os códigos bloqueados: a lista que a campainha da sala lê.
    #[must_use]
    pub fn bloqueados(&self) -> Vec<Codigo> {
        self.contatos
            .iter()
            .filter(|contato| contato.bloqueado)
            .map(|contato| contato.codigo)
            .collect()
    }

    /// Muda a linha deste código, criada vazia se não havia, e grava.
    fn mudar(&mut self, codigo: Codigo, mudanca: impl FnOnce(&mut Contato)) -> Result<()> {
        match self.contatos.iter_mut().find(|c| c.codigo == codigo) {
            Some(contato) => mudanca(contato),
            None => {
                let mut novo = Contato {
                    codigo,
                    nome: None,
                    apelido: None,
                    visto_em: None,
                    direcao: None,
                    bloqueado: false,
                };
                mudanca(&mut novo);
                self.contatos.push(novo);
            }
        }
        self.gravar()
    }

    fn gravar(&self) -> Result<()> {
        let texto: String = self
            .contatos
            .iter()
            .map(|c| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\n",
                    c.codigo,
                    c.nome.as_deref().unwrap_or(""),
                    c.apelido.as_deref().unwrap_or(""),
                    c.visto_em.map_or_else(String::new, |v| v.to_string()),
                    c.direcao.map_or("", Direcao::texto),
                    u8::from(c.bloqueado),
                )
            })
            .collect();
        crate::conhecidos::escrever_privado(&self.caminho, texto.as_bytes())
            .with_context(|| format!("não consegui gravar {}", self.caminho.display()))
    }
}

/// Uma linha do arquivo, ou nada se ela não se lê.
fn ler_linha(linha: &str) -> Option<Contato> {
    let mut campos = linha.split('\t');
    let codigo = Codigo::ler(campos.next()?).ok()?;
    let texto = |campo: Option<&str>| campo.map(str::to_owned).filter(|t| !t.is_empty());
    let nome = texto(campos.next());
    let apelido = texto(campos.next());
    let visto_em = campos.next().and_then(|v| v.parse().ok());
    let direcao = campos.next().and_then(Direcao::ler);
    let bloqueado = campos.next() == Some("1");
    Some(Contato {
        codigo,
        nome,
        apelido,
        visto_em,
        direcao,
        bloqueado,
    })
}

/// Um texto que cabe numa coluna: todo caractere de controle, tabulação e
/// quebra inclusive, vira espaço. O apelido vem da rede, e um `\n` nele
/// abriria uma linha nova com o que o outro lado quisesse escrever.
fn limpo(texto: &str) -> String {
    texto
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codigo(n: u8) -> Codigo {
        Codigo::da_chave(&[n; 32])
    }

    /// Um arquivo de agenda numa pasta só deste teste. O `seele-core` não tem
    /// `tempfile`, e `conhecidos.rs` faz o mesmo: o pid e o nome do teste na
    /// pasta, para dois testes em paralelo não se encontrarem.
    fn nova(nome: &str) -> PathBuf {
        let pasta =
            std::env::temp_dir().join(format!("seele-agenda-{}-{nome}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        pasta.join("agenda")
    }

    #[test]
    fn uma_ligacao_anotada_volta_ao_reabrir() {
        let caminho = nova("volta");
        let mut agenda = Agenda::abrir(caminho.clone()).unwrap();
        agenda
            .anotar_ligacao(codigo(2), "rafa", Direcao::Recebi, 1_738_000_000)
            .unwrap();

        let de_novo = Agenda::abrir(caminho).unwrap();
        assert_eq!(
            de_novo.buscar(&codigo(2)),
            Some(&Contato {
                codigo: codigo(2),
                nome: None,
                apelido: Some("rafa".into()),
                visto_em: Some(1_738_000_000),
                direcao: Some(Direcao::Recebi),
                bloqueado: false,
            }),
            "a ligação anotada não sobreviveu ao reabrir"
        );
    }

    #[test]
    fn o_nome_que_dei_nao_e_trocado_pelo_apelido_que_veio() {
        let caminho = nova("nome");
        let mut agenda = Agenda::abrir(caminho).unwrap();
        agenda.dar_nome(codigo(2), "Rafa da escola").unwrap();
        agenda
            .anotar_ligacao(codigo(2), "outro nome", Direcao::Perdida, 5)
            .unwrap();
        let contato = agenda.buscar(&codigo(2)).unwrap();
        assert_eq!(
            contato.nome.as_deref(),
            Some("Rafa da escola"),
            "o apelido que veio do outro lado trocou o nome que eu dei"
        );
        assert_eq!(contato.apelido.as_deref(), Some("outro nome"));
        assert_eq!(contato.direcao, Some(Direcao::Perdida));
    }

    #[test]
    fn um_apelido_com_tabulacao_ou_quebra_nao_quebra_o_arquivo() {
        let caminho = nova("tabulacao");
        let mut agenda = Agenda::abrir(caminho.clone()).unwrap();
        agenda
            .anotar_ligacao(codigo(2), "a\tb\nc", Direcao::Recebi, 1)
            .unwrap();
        agenda
            .anotar_ligacao(codigo(3), "ok", Direcao::Liguei, 2)
            .unwrap();

        let de_novo = Agenda::abrir(caminho).unwrap();
        assert_eq!(
            de_novo.listar().len(),
            2,
            "um apelido vindo da rede abriu uma linha nova no arquivo"
        );
        assert_eq!(
            de_novo.buscar(&codigo(2)).unwrap().apelido.as_deref(),
            Some("a b c")
        );
    }

    #[test]
    fn uma_linha_ilegivel_ou_de_codigo_errado_e_pulada_e_as_outras_ficam() {
        let caminho = nova("ilegivel");
        std::fs::create_dir_all(caminho.parent().unwrap()).unwrap();
        let valido = codigo(2).to_string();
        // O mesmo código com o último símbolo trocado: o verificador não bate.
        let mut torto = valido.clone();
        let ultimo = torto.pop().unwrap();
        torto.push(if ultimo == '0' { '1' } else { '0' });
        std::fs::write(
            &caminho,
            format!(
                "lixo sem tabulação\n{torto}\t\tx\t1\trecebi\t0\n{valido}\t\trafa\t1\trecebi\t0\n"
            ),
        )
        .unwrap();

        let agenda = Agenda::abrir(caminho).unwrap();
        assert_eq!(
            agenda.listar().iter().map(|c| c.codigo).collect::<Vec<_>>(),
            vec![codigo(2)],
            "a agenda não pulou o lixo, ou perdeu a linha boa"
        );
    }

    #[test]
    fn bloquear_poe_o_codigo_na_lista_da_campainha_e_sobrevive_ao_reabrir() {
        let caminho = nova("bloqueio");
        let mut agenda = Agenda::abrir(caminho.clone()).unwrap();
        agenda.bloquear(codigo(9), true).unwrap();
        assert!(
            agenda.buscar(&codigo(9)).is_some(),
            "bloquear quem nunca ligou não criou a linha"
        );
        assert_eq!(agenda.bloqueados(), vec![codigo(9)]);

        let mut de_novo = Agenda::abrir(caminho).unwrap();
        assert_eq!(
            de_novo.bloqueados(),
            vec![codigo(9)],
            "o bloqueio se perdeu ao reabrir"
        );
        de_novo.bloquear(codigo(9), false).unwrap();
        assert!(
            de_novo.bloqueados().is_empty(),
            "desbloquear não tirou da lista"
        );
    }

    #[cfg(unix)]
    #[test]
    fn o_arquivo_nasce_so_para_o_dono() {
        use std::os::unix::fs::PermissionsExt;
        let caminho = nova("permissao");
        let mut agenda = Agenda::abrir(caminho.clone()).unwrap();
        agenda.bloquear(codigo(2), true).unwrap();
        let modo = std::fs::metadata(&caminho).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            modo, 0o600,
            "a agenda ficou legível para outras contas desta máquina"
        );
    }
}
