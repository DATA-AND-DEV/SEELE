//! A tela de onde se escolhe para onde ir.
//!
//! `connection` sem argumento nenhum cai aqui: os servidores onde você já esteve, mais
//! *novo endereço*, *colar convite* e *hospedar aqui*. Com qualquer flag, esta
//! tela não aparece — quem digitou `connection --server casa:8383` já disse aonde
//! vai, e um menu no caminho seria só uma tecla a mais entre a intenção e o
//! resultado. O público deste cliente é o mesmo do Vim; interromper quem sabe o
//! que quer é o pior que uma interface faz.
//!
//! # Isto não é um sétimo estado
//!
//! `specs/05-cliente-tui.md` descreve seis estados visuais, e todos os seis
//! descrevem uma **sessão**. Esta tela roda antes de existir sessão: não há
//! roster, nem telemetria, nem Taxa de Sincronização, e nada aqui pode ser
//! expresso em [`crate::app::Screen`] sem inventar campos vazios para os outros
//! cinco. Por isso mora fora do `App`, com estado próprio, e some quando a
//! conexão começa.
//!
//! # Onde estão as decisões
//!
//! [`Selecao`] é uma máquina de estados pura: entra tecla, sai
//! [`Resultado`]. Não toca no terminal, não conecta, não lê disco. O laço e o
//! desenho estão nas duas funções do fim do arquivo, e é o que permite testar
//! "digitar um endereço e apertar Enter" sem um terminal.

use crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use seele_core::conhecidos::Conhecido;

use crate::theme::{self, Theme};
use crate::ui;

/// O que a tela devolve quando alguém escolhe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escolha {
    /// `host:porta`, como será digitado no `--server`.
    pub alvo: String,
    /// Com que nome entrar.
    pub apelido: String,
    /// voice room a entrar.
    pub voice_room: u32,
    /// Convite de uso único, quando veio de um link.
    pub convite: Option<String>,
    /// Impressão digital esperada, quando veio de um link.
    pub impressao_digital: Option<String>,
    /// Subir o servidor neste processo em vez de procurar um.
    pub hospedar: bool,
}

/// O que uma tecla causou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resultado {
    /// Continua na tela.
    Segue,
    /// Escolheu. Conecta.
    Pronto(Box<Escolha>),
    /// Desistiu.
    Sai,
    /// Pediu para esquecer um servidor da lista.
    ///
    /// Quem chamou é que sabe gravar em disco; esta máquina não escreve nada.
    Esquecer(String),
}

/// Uma linha da lista.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    /// Um servidor já visitado.
    Visitado(Conhecido),
    /// Digitar um endereço.
    Novo,
    /// Colar um `seele://`.
    Colar,
    /// Hospedar aqui mesmo.
    Hospedar,
}

/// Qual pergunta está aberta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pergunta {
    Endereco,
    Link,
    Apelido,
}

/// A tela.
#[derive(Debug, Clone)]
pub struct Selecao {
    itens: Vec<Item>,
    cursor: usize,
    /// A pergunta aberta, se houver, e o que já foi digitado nela.
    aberta: Option<(Pergunta, String)>,
    /// O que já se juntou sobre o destino.
    rascunho: Escolha,
    /// Um recado — link inválido, normalmente.
    aviso: Option<String>,
}

/// Quando não se sabe o apelido de ninguém.
const APELIDO_PADRAO: &str = "pessoa";
/// Onde um servidor hospedado aqui escuta.
const AQUI: &str = "127.0.0.1:8383";

impl Selecao {
    /// Monta a tela a partir do que se sabe.
    ///
    /// A lista já vem do mais recente para o mais antigo, e o cursor começa na
    /// primeira linha: apertar Enter sem ler nada leva de volta aonde você
    /// estava por último, que é o que quase sempre se quer.
    #[must_use]
    pub fn nova(visitados: Vec<Conhecido>) -> Self {
        // O último apelido usado vira o padrão. Ninguém troca de nome entre
        // uma conversa e outra, e redigitar seria só trabalho.
        let apelido = visitados
            .first()
            .map_or_else(|| APELIDO_PADRAO.to_owned(), |c| c.apelido.clone());

        let mut itens: Vec<Item> = visitados.into_iter().map(Item::Visitado).collect();
        itens.push(Item::Novo);
        itens.push(Item::Colar);
        itens.push(Item::Hospedar);

        Self {
            itens,
            cursor: 0,
            aberta: None,
            rascunho: Escolha {
                alvo: String::new(),
                apelido,
                voice_room: 1,
                convite: None,
                impressao_digital: None,
                hospedar: false,
            },
            aviso: None,
        }
    }

    /// Uma tecla.
    pub fn tecla(&mut self, tecla: KeyCode) -> Resultado {
        // Um aviso vale até a próxima tecla. Deixá-lo na tela depois disso o
        // transformaria em decoração.
        self.aviso = None;

        if self.aberta.is_some() {
            return self.tecla_no_campo(tecla);
        }
        self.tecla_na_lista(tecla)
    }

    fn tecla_na_lista(&mut self, tecla: KeyCode) -> Resultado {
        match tecla {
            KeyCode::Char('j') | KeyCode::Down => {
                self.cursor = (self.cursor + 1).min(self.itens.len().saturating_sub(1));
                Resultado::Segue
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                Resultado::Segue
            }
            // Atalhos diretos, para quem já sabe o que quer.
            KeyCode::Char('n') => self.ir_para(&Item::Novo),
            KeyCode::Char('c') => self.ir_para(&Item::Colar),
            KeyCode::Char('h') => self.ir_para(&Item::Hospedar),
            KeyCode::Char('d') => self.esquecer_o_do_cursor(),
            KeyCode::Enter => match self.itens.get(self.cursor).cloned() {
                Some(item) => self.ir_para(&item),
                None => Resultado::Segue,
            },
            KeyCode::Char('q') | KeyCode::Esc => Resultado::Sai,
            _ => Resultado::Segue,
        }
    }

    fn ir_para(&mut self, item: &Item) -> Resultado {
        match item {
            // Um servidor conhecido já tem tudo: endereço, apelido e a sala de voz de
            // onde se parou. Uma tecla e pronto — é para isso que a lista
            // existe.
            Item::Visitado(conhecido) => {
                self.apontar_para(conhecido.alvo.clone());
                self.rascunho.apelido.clone_from(&conhecido.apelido);
                self.rascunho.voice_room = conhecido.voice_room.unwrap_or(1);
                Resultado::Pronto(Box::new(self.rascunho.clone()))
            }
            Item::Novo => {
                self.aberta = Some((Pergunta::Endereco, String::new()));
                Resultado::Segue
            }
            Item::Colar => {
                self.aberta = Some((Pergunta::Link, String::new()));
                Resultado::Segue
            }
            Item::Hospedar => {
                self.rascunho.hospedar = true;
                self.apontar_para(AQUI.to_owned());
                self.perguntar_apelido();
                Resultado::Segue
            }
        }
    }

    /// Aponta o rascunho para outro endereço, largando o que era do anterior.
    ///
    /// Um convite vale para o servidor dele e para mais nenhum. A impressão
    /// digital e o token vieram de um link que fala de um endereço só, e
    /// arrastá-los para o servidor seguinte produz as duas piores telas que este
    /// cliente tem: contra um servidor ainda não fixado, uma recusa
    /// ("ESTE NÃO É O SERVIDOR DO CONVITE") que ninguém pediu e que não há como
    /// desfazer sem reiniciar; contra um já fixado, uma acusação laranja contra
    /// o servidor de todo dia. O token faz o par disso do outro lado — um servidor
    /// que nunca pediu credencial nenhuma responderia `CredentialRejected`.
    ///
    /// O app já tem esta disciplina (`apps/seele-app/src/main.rs` descarta o
    /// convite guardado quando o endereço no campo não é o dele, e
    /// `specs/06-clientes-gui.md` a descreve como comportamento de produto);
    /// aqui era a única casca sem ela, porque o Esc na pergunta do apelido
    /// preserva o rascunho de propósito.
    ///
    /// Voltar ao **mesmo** endereço não larga nada: aí o convite continua sendo
    /// daquele servidor, e limpá-lo seria perder a conferência que o link existe
    /// para fazer.
    fn apontar_para(&mut self, alvo: String) {
        if self.rascunho.alvo != alvo {
            self.rascunho.impressao_digital = None;
            self.rascunho.convite = None;
        }
        self.rascunho.alvo = alvo;
    }

    fn esquecer_o_do_cursor(&mut self) -> Resultado {
        let Some(Item::Visitado(conhecido)) = self.itens.get(self.cursor) else {
            return Resultado::Segue;
        };
        let alvo = conhecido.alvo.clone();
        self.itens.remove(self.cursor);
        self.cursor = self.cursor.min(self.itens.len().saturating_sub(1));
        Resultado::Esquecer(alvo)
    }

    /// Abre a pergunta do apelido, vazia, com o último usado como sugestão.
    ///
    /// **Vazia**, e não preenchida com a sugestão. Preencher parece atencioso e
    /// é uma armadilha: quem quer outro nome digita, e o que digita gruda no que
    /// já estava. Foi assim que o primeiro teste desta tela entrou num servidor
    /// como `pessoacarla`. Sugestão se mostra apagada e se aceita com Enter;
    /// digitar escreve por cima.
    fn perguntar_apelido(&mut self) {
        self.aberta = Some((Pergunta::Apelido, String::new()));
    }

    fn tecla_no_campo(&mut self, tecla: KeyCode) -> Resultado {
        let Some((pergunta, valor)) = self.aberta.as_mut() else {
            return Resultado::Segue;
        };

        match tecla {
            KeyCode::Char(letra) => {
                valor.push(letra);
                Resultado::Segue
            }
            KeyCode::Backspace => {
                valor.pop();
                Resultado::Segue
            }
            // Esc volta para a lista sem perder o que já foi juntado antes.
            KeyCode::Esc => {
                self.aberta = None;
                self.rascunho.hospedar = false;
                Resultado::Segue
            }
            KeyCode::Enter => {
                let pergunta = *pergunta;
                let resposta = valor.trim().to_owned();
                self.responder(pergunta, &resposta)
            }
            _ => Resultado::Segue,
        }
    }

    fn responder(&mut self, pergunta: Pergunta, resposta: &str) -> Resultado {
        match pergunta {
            Pergunta::Endereco => {
                if resposta.is_empty() {
                    self.aviso = Some("um endereço, ou Esc para voltar".to_owned());
                    return Resultado::Segue;
                }
                self.apontar_para(com_porta(resposta));
                self.perguntar_apelido();
                Resultado::Segue
            }

            Pergunta::Link => match seele_core::uri::analisar(resposta) {
                Ok(convite) => {
                    self.apontar_para(com_porta(&convite.alvo));
                    self.rascunho.impressao_digital = convite.impressao_digital;
                    self.rascunho.convite = convite.token;
                    if let Some(numero) = convite.voice_room {
                        self.rascunho.voice_room = numero;
                    }
                    self.perguntar_apelido();
                    Resultado::Segue
                }
                // O link fica no campo para poder ser corrigido. Limpá-lo
                // obrigaria a colar de novo por causa de um caractere.
                Err(erro) => {
                    self.aviso = Some(format!("link inválido: {erro}"));
                    Resultado::Segue
                }
            },

            // Campo vazio é aceitar a sugestão, que é o último apelido usado —
            // ou o padrão, na primeira vez.
            Pergunta::Apelido => {
                if !resposta.is_empty() {
                    self.rascunho.apelido = resposta.to_owned();
                }
                if self.rascunho.apelido.is_empty() {
                    self.rascunho.apelido = APELIDO_PADRAO.to_owned();
                }
                self.aberta = None;
                Resultado::Pronto(Box::new(self.rascunho.clone()))
            }
        }
    }
}

/// `casa` vira `casa:8383`; `casa:9000` fica como está.
///
/// Um endereço IPv6 sem colchetes é ambíguo e cai no caso de ter porta, o que
/// dá um erro de resolução legível em vez de um endereço remendado errado.
fn com_porta(alvo: &str) -> String {
    if alvo.contains(':') {
        alvo.to_owned()
    } else {
        format!("{alvo}:8383")
    }
}

// ------------------------------------------------------------------ desenho

/// Desenha a tela.
pub fn desenhar(frame: &mut Frame<'_>, selecao: &Selecao, tema: Theme) {
    let area = frame.area();
    frame.render_widget(
        ratatui::widgets::Block::default().style(tema.background()),
        area,
    );

    // `docs/marca.md` dá à TUI a variação em texto puro, e ela vem no título
    // porque o quadro é o único lugar desta tela que não divide linha com
    // dado. Os três caracteres da marca são de largura ambígua: em terminal
    // que os desenha em célula dupla o título ocupa o dobro, e é o título que
    // pode dar essa margem sem desalinhar coisa nenhuma.
    let titulo = format!(" {} ", ui::ASSINATURA);
    let bloco = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(tema.border(true))
        .title(Span::styled(titulo, tema.accent()));
    let dentro = bloco.inner(area);
    frame.render_widget(bloco, area);

    let [cabecalho, corpo, rodape] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .areas(dentro);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(format!("  {}", ui::ASSINATURA), tema.accent())),
            Line::from(Span::styled("  para onde?", tema.label())),
        ]),
        cabecalho,
    );

    if let Some((pergunta, valor)) = &selecao.aberta {
        desenhar_campo(frame, selecao, tema, corpo, *pergunta, valor);
    } else {
        desenhar_lista(frame, selecao, tema, corpo);
    }

    let dica = if selecao.aberta.is_some() {
        "Enter confirma · Esc volta"
    } else {
        "j/k move · Enter entra · n novo · c colar convite · h hospedar · d esquece · q sai"
    };
    let mut linhas = vec![Line::from(Span::styled(
        format!(
            "  {}",
            ui::truncate(dica, rodape.width.saturating_sub(2) as usize)
        ),
        tema.label(),
    ))];
    if let Some(aviso) = &selecao.aviso {
        linhas.push(Line::from(Span::styled(
            format!(
                "  {}",
                ui::truncate(aviso, rodape.width.saturating_sub(2) as usize)
            ),
            tema.alert(),
        )));
    }
    frame.render_widget(Paragraph::new(linhas), rodape);
}

fn desenhar_lista(frame: &mut Frame<'_>, selecao: &Selecao, tema: Theme, area: Rect) {
    let largura = area.width.saturating_sub(4) as usize;
    let linhas: Vec<Line<'_>> = selecao
        .itens
        .iter()
        .enumerate()
        .map(|(indice, item)| {
            let marcado = indice == selecao.cursor;
            // A marca é o que diz onde está o cursor sem depender de cor —
            // `specs/05` não admite informação transmitida só por cor.
            let marca = if marcado { "▸ " } else { "  " };
            let estilo = if marcado { tema.accent() } else { tema.body() };

            let texto = match item {
                Item::Visitado(conhecido) => {
                    let voice_room = conhecido
                        .voice_room
                        .map_or_else(String::new, |c| format!("  · SALA {c}"));
                    format!("{}  ({}){}", conhecido.alvo, conhecido.apelido, voice_room)
                }
                Item::Novo => "novo endereço…".to_owned(),
                Item::Colar => "colar um convite seele://…".to_owned(),
                Item::Hospedar => "hospedar aqui neste computador".to_owned(),
            };

            Line::from(vec![
                Span::styled(format!("  {marca}"), estilo),
                Span::styled(ui::truncate(&texto, largura), estilo),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(linhas), area);
}

fn desenhar_campo(
    frame: &mut Frame<'_>,
    selecao: &Selecao,
    tema: Theme,
    area: Rect,
    pergunta: Pergunta,
    valor: &str,
) {
    let (rotulo, exemplo) = match pergunta {
        Pergunta::Endereco => (
            "endereço do servidor",
            "192.168.0.7:8383  ou  casa.exemplo:8383",
        ),
        Pergunta::Link => ("cole o convite", "seele://192.168.0.7:8383?fp=…"),
        Pergunta::Apelido => ("seu nome no roster", "como os outros vão te ver"),
    };

    let mut campo = vec![
        Span::styled("  ", tema.body()),
        // O bloco é o cursor. Um campo sem cursor visível parece travado.
        Span::styled(
            ui::truncate(valor, area.width.saturating_sub(6) as usize),
            tema.body(),
        ),
        Span::styled("█", tema.fg(theme::NERV)),
    ];

    // A sugestão fica apagada **depois** do cursor enquanto o campo está vazio,
    // e some na primeira letra. É o que a torna sugestão em vez de um texto já
    // digitado que ninguém pediu — e digitar não gruda no que estava lá.
    if pergunta == Pergunta::Apelido && valor.is_empty() {
        campo.push(Span::styled(
            format!("{}  ← Enter aceita", selecao.rascunho.apelido),
            tema.label(),
        ));
    }

    let mut linhas = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {rotulo}"), tema.label())),
        Line::from(""),
        Line::from(campo),
        Line::from(""),
        Line::from(Span::styled(format!("  {exemplo}"), tema.label())),
    ];

    if selecao.rascunho.hospedar {
        linhas.push(Line::from(""));
        linhas.push(Line::from(Span::styled(
            "  o servidor sobe aqui e acaba quando você sair",
            tema.label(),
        )));
    }

    frame.render_widget(Paragraph::new(linhas).alignment(Alignment::Left), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conhecido(alvo: &str, apelido: &str, voice_room: Option<u32>) -> Conhecido {
        Conhecido {
            alvo: alvo.to_owned(),
            apelido: apelido.to_owned(),
            voice_room,
            visto_em: 0,
            // Os dois campos que a lista de conhecidos ganhou depois: o nome do
            // servidor e o ícone dele. Nenhum teste desta tela olha para eles — o
            // que se testa aqui é a busca e a seleção, que trabalham sobre
            // `alvo` e `apelido` —, e `None` é o que uma entrada escrita antes
            // de os campos existirem carrega, que é o caso mais comum em disco.
            nome: None,
            icone: None,
        }
    }

    fn digitar(selecao: &mut Selecao, texto: &str) {
        for letra in texto.chars() {
            assert_eq!(selecao.tecla(KeyCode::Char(letra)), Resultado::Segue);
        }
    }

    #[test]
    fn enter_no_primeiro_item_volta_para_onde_se_estava() {
        // A lista chega ordenada do mais recente para o mais antigo, e o cursor
        // começa em cima. Uma tecla para voltar à última conversa.
        let mut selecao = Selecao::nova(vec![
            conhecido("recente:8383", "marcela", Some(2)),
            conhecido("antigo:8383", "rei", None),
        ]);

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert_eq!(escolha.alvo, "recente:8383");
        assert_eq!(escolha.apelido, "marcela");
        assert_eq!(escolha.voice_room, 2);
        assert!(!escolha.hospedar);
    }

    #[test]
    fn um_endereco_novo_pergunta_o_apelido_e_sugere_o_ultimo() {
        // Ninguém troca de nome entre uma conversa e outra. Enter aceita.
        let mut selecao = Selecao::nova(vec![conhecido("casa:8383", "marcela", None)]);

        assert_eq!(selecao.tecla(KeyCode::Char('n')), Resultado::Segue);
        digitar(&mut selecao, "192.168.0.9");
        assert_eq!(selecao.tecla(KeyCode::Enter), Resultado::Segue);

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert_eq!(escolha.alvo, "192.168.0.9:8383", "não completou a porta");
        assert_eq!(escolha.apelido, "marcela", "não sugeriu o último apelido");
    }

    #[test]
    fn digitar_um_apelido_escreve_por_cima_da_sugestao() {
        // Achado dirigindo a tela de verdade: com o campo preenchido pela
        // sugestão, digitar `carla` entrava no servidor como `pessoacarla`.
        let mut selecao = Selecao::nova(vec![conhecido("casa:8383", "pessoa", None)]);

        selecao.tecla(KeyCode::Char('n'));
        digitar(&mut selecao, "novo:8383");
        selecao.tecla(KeyCode::Enter);
        digitar(&mut selecao, "carla");

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert_eq!(escolha.apelido, "carla", "o nome grudou na sugestão");
    }

    #[test]
    fn um_convite_colado_traz_impressao_digital_e_token() {
        // É o caminho de quem recebeu um link no chat. Se a impressão digital
        // se perdesse aqui, o primeiro contato voltaria a ser cego.
        let link =
            "seele://192.168.0.7:8383?fp=".to_owned() + &"ab".repeat(32) + "&convite=A1B2C3D4";
        let mut selecao = Selecao::nova(Vec::new());

        assert_eq!(selecao.tecla(KeyCode::Char('c')), Resultado::Segue);
        digitar(&mut selecao, &link);
        assert_eq!(selecao.tecla(KeyCode::Enter), Resultado::Segue);

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert_eq!(escolha.alvo, "192.168.0.7:8383");
        assert_eq!(escolha.impressao_digital, Some("ab".repeat(32)));
        assert_eq!(escolha.convite, Some("A1B2C3D4".to_owned()));
        assert_eq!(escolha.apelido, APELIDO_PADRAO);
    }

    /// Um convite de exemplo, com impressão digital e token.
    fn link_de_teste(alvo: &str) -> String {
        format!("seele://{alvo}?fp={}&convite=A1B2C3D4", "ab".repeat(32))
    }

    #[test]
    fn o_convite_colado_nao_segue_para_outro_server() {
        // O caminho mais curto até o defeito, e o que o reproduziu: colar um
        // convite, apertar Esc na pergunta do apelido — que preserva o rascunho
        // de propósito — e entrar num server da lista. A impressão digital ia
        // junto, e `connection` acusava de impostor um servidor que o convite nem cita.
        let mut selecao = Selecao::nova(vec![conhecido("outro:8383", "marcela", None)]);

        selecao.tecla(KeyCode::Char('c'));
        digitar(&mut selecao, &link_de_teste("server-do-link:8383"));
        assert_eq!(selecao.tecla(KeyCode::Enter), Resultado::Segue);
        assert_eq!(selecao.tecla(KeyCode::Esc), Resultado::Segue);

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert_eq!(escolha.alvo, "outro:8383");
        assert_eq!(
            escolha.impressao_digital, None,
            "a impressão do link foi cobrada de um servidor que ele não menciona"
        );
        assert_eq!(
            escolha.convite, None,
            "o token foi mandado a um servidor que não pediu credencial nenhuma"
        );
    }

    #[test]
    fn digitar_outro_endereco_depois_do_convite_tambem_larga_o_que_era_dele() {
        // Mesmo defeito pela porta ao lado: Esc, `n`, e um endereço à mão.
        let mut selecao = Selecao::nova(Vec::new());

        selecao.tecla(KeyCode::Char('c'));
        digitar(&mut selecao, &link_de_teste("server-do-link:8383"));
        selecao.tecla(KeyCode::Enter);
        selecao.tecla(KeyCode::Esc);

        selecao.tecla(KeyCode::Char('n'));
        digitar(&mut selecao, "outro:8383");
        assert_eq!(selecao.tecla(KeyCode::Enter), Resultado::Segue);

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert_eq!(escolha.alvo, "outro:8383");
        assert_eq!(escolha.impressao_digital, None);
        assert_eq!(escolha.convite, None);
    }

    #[test]
    fn hospedar_depois_de_um_convite_nao_confere_o_convite_contra_si_mesmo() {
        // O servidor que sobe aqui é deste computador; um convite de outro lugar
        // não tem nada a dizer sobre ele.
        let mut selecao = Selecao::nova(Vec::new());

        selecao.tecla(KeyCode::Char('c'));
        digitar(&mut selecao, &link_de_teste("server-do-link:8383"));
        selecao.tecla(KeyCode::Enter);
        selecao.tecla(KeyCode::Esc);

        selecao.tecla(KeyCode::Char('h'));
        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert_eq!(escolha.alvo, AQUI);
        assert_eq!(escolha.impressao_digital, None);
        assert_eq!(escolha.convite, None);
    }

    #[test]
    fn voltar_ao_mesmo_endereco_do_convite_mantem_a_conferencia() {
        // O outro lado da regra: o convite é daquele servidor, e continua sendo
        // dele. Limpá-lo aqui perderia a conferência que o link existe para
        // fazer — que é o defeito oposto, e igualmente caro.
        let mut selecao = Selecao::nova(vec![conhecido("server-do-link:8383", "marcela", None)]);

        selecao.tecla(KeyCode::Char('c'));
        digitar(&mut selecao, &link_de_teste("server-do-link:8383"));
        selecao.tecla(KeyCode::Enter);
        selecao.tecla(KeyCode::Esc);

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert_eq!(escolha.alvo, "server-do-link:8383");
        assert_eq!(escolha.impressao_digital, Some("ab".repeat(32)));
        assert_eq!(escolha.convite, Some("A1B2C3D4".to_owned()));
    }

    #[test]
    fn um_link_torto_avisa_e_deixa_corrigir() {
        // Limpar o campo obrigaria a colar de novo por causa de um caractere.
        let mut selecao = Selecao::nova(Vec::new());
        assert_eq!(selecao.tecla(KeyCode::Char('c')), Resultado::Segue);
        digitar(&mut selecao, "http://nao-e-um-convite");

        assert_eq!(selecao.tecla(KeyCode::Enter), Resultado::Segue);
        assert!(selecao.aviso.is_some(), "não avisou");
        assert_eq!(
            selecao.aberta,
            Some((Pergunta::Link, "http://nao-e-um-convite".to_owned())),
            "apagou o que a pessoa colou"
        );
    }

    #[test]
    fn o_aviso_some_na_tecla_seguinte() {
        // Um aviso que fica vira decoração, e decoração não é lida.
        let mut selecao = Selecao::nova(Vec::new());
        selecao.tecla(KeyCode::Char('c'));
        digitar(&mut selecao, "torto");
        selecao.tecla(KeyCode::Enter);
        assert!(selecao.aviso.is_some());

        selecao.tecla(KeyCode::Backspace);
        assert!(selecao.aviso.is_none());
    }

    #[test]
    fn hospedar_escolhe_o_proprio_computador() {
        let mut selecao = Selecao::nova(Vec::new());
        assert_eq!(selecao.tecla(KeyCode::Char('h')), Resultado::Segue);

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert!(escolha.hospedar);
        assert_eq!(escolha.alvo, AQUI);
    }

    #[test]
    fn esc_no_campo_desfaz_a_intencao_de_hospedar() {
        // Sem isto, entrar em "hospedar", voltar, e escolher outro servidor subiria
        // um servidor que ninguém pediu.
        let mut selecao = Selecao::nova(vec![conhecido("casa:8383", "marcela", None)]);
        selecao.tecla(KeyCode::Char('h'));
        selecao.tecla(KeyCode::Esc);

        let Resultado::Pronto(escolha) = selecao.tecla(KeyCode::Enter) else {
            panic!("não escolheu");
        };
        assert!(!escolha.hospedar, "subiria um servidor sem ninguém pedir");
        assert_eq!(escolha.alvo, "casa:8383");
    }

    #[test]
    fn d_esquece_um_conhecido_e_nao_toca_nas_acoes() {
        let mut selecao = Selecao::nova(vec![conhecido("casa:8383", "marcela", None)]);

        assert_eq!(
            selecao.tecla(KeyCode::Char('d')),
            Resultado::Esquecer("casa:8383".to_owned())
        );
        // Sobraram as três ações, e `d` em cima de uma delas não faz nada.
        assert_eq!(selecao.itens.len(), 3);
        assert_eq!(selecao.tecla(KeyCode::Char('d')), Resultado::Segue);
        assert_eq!(selecao.itens.len(), 3);
    }

    #[test]
    fn o_cursor_nao_sai_da_lista() {
        let mut selecao = Selecao::nova(Vec::new());
        for _ in 0..10 {
            selecao.tecla(KeyCode::Char('j'));
        }
        assert_eq!(selecao.cursor, selecao.itens.len() - 1);
        for _ in 0..10 {
            selecao.tecla(KeyCode::Char('k'));
        }
        assert_eq!(selecao.cursor, 0);
    }

    #[test]
    fn q_sai_da_lista_mas_digita_dentro_do_campo() {
        // `q` é sair na lista e é a letra q num campo de texto. Confundir os
        // dois faria "quarto" fechar o programa no meio da palavra.
        let mut selecao = Selecao::nova(Vec::new());
        assert_eq!(selecao.tecla(KeyCode::Char('n')), Resultado::Segue);
        assert_eq!(selecao.tecla(KeyCode::Char('q')), Resultado::Segue);
        assert_eq!(selecao.aberta, Some((Pergunta::Endereco, "q".to_owned())));

        selecao.tecla(KeyCode::Esc);
        assert_eq!(selecao.tecla(KeyCode::Char('q')), Resultado::Sai);
    }

    #[test]
    fn a_tela_cabe_no_terminal_minimo() {
        // 80×24 é o piso de `specs/05`, e uma tela de entrada que estoura o
        // piso é uma tela que ninguém consegue usar para entrar.
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let selecao = Selecao::nova(vec![conhecido("192.168.0.7:8383", "marcela", Some(1))]);
        let mut terminal =
            Terminal::new(TestBackend::new(ui::MIN_WIDTH, ui::MIN_HEIGHT)).expect("terminal");
        terminal
            .draw(|frame| {
                desenhar(
                    frame,
                    &selecao,
                    Theme::with_palette(crate::theme::Palette::True),
                )
            })
            .expect("desenhar");

        let buffer = terminal.backend().buffer();
        let tudo: String = (0..ui::MIN_HEIGHT)
            .flat_map(|y| (0..ui::MIN_WIDTH).map(move |x| (x, y)))
            .map(|posicao| buffer[posicao].symbol().to_owned())
            .collect();
        // O cabeçalho é a marca da folha nova (`docs/marca.md`), não mais a
        // assinatura do connection de entrada, que foi abandonada com o katakana.
        assert!(tudo.contains(ui::ASSINATURA));
        assert!(tudo.contains("192.168.0.7:8383"));
        assert!(tudo.contains("hospedar"));
    }
}
