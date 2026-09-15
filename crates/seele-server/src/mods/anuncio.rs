//! O que este servidor exige de quem entra, montado da tabela.
//!
//! ADR 0045: «na primeira vez que entra num servidor com MODs, a pessoa vê o
//! que vai baixar — nome, autor, versão, repositório, e o `reach` declarado»,
//! e «quem não aceita, não entra».
//!
//! Este módulo é a metade do servidor dessa frase, e só ela: montar o conjunto
//! e dizer quando não dá. Quem manda pelo fio e quem espera a resposta é o
//! `session`; quem guarda o aceite é o cliente.
//!
//! # Por que ele pode falhar, e por que a falha não é silenciosa
//!
//! Uma linha habilitada é uma **exigência**: quem entra tem de baixar aquele
//! MOD e conferir aquele hash. Se a linha não descreve nada que se possa
//! conferir — um hash que não é um hash, MODs demais para um quadro —, o
//! servidor não consegue dizer o que exige.
//!
//! Admitir mesmo assim seria exigir na tabela e não exigir no fio: a pessoa
//! entraria sem MOD nenhum num servidor que se declara com MOD, que é a
//! instalação parcial silenciosa que o ADR 0029 nomeia como o modo de falha a
//! evitar e o 0045 herda inteira.
//!
//! Então ninguém entra, e quem hospeda lê **qual** linha e **por quê** — que é
//! a única parte que ele pode consertar.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use seele_proto::control::{ServerMessage, Validate};
use seele_proto::mods::{e_hash_de_conteudo, hex, identidade_do_conjunto, ModAnunciado};

use crate::persistence::Persistence;

/// O conjunto de MODs que este servidor exige agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conjunto {
    /// Os MODs habilitados, já na forma que atravessa o fio.
    pub mods: Vec<ModAnunciado>,
    /// A identidade do conjunto inteiro, em hexadecimal minúsculo.
    ///
    /// É contra ela que o aceite de quem entra é conferido, e é ela que muda
    /// quando o servidor troca de MOD.
    pub identidade: String,
}

impl Conjunto {
    /// Se este servidor não exige MOD nenhum.
    ///
    /// **O caso normal, e é o que preserva o comportamento de antes:** um
    /// servidor sem MOD não manda anúncio, não espera resposta, e troca
    /// exatamente os quadros que trocava.
    #[must_use]
    pub fn vazio(&self) -> bool {
        self.mods.is_empty()
    }

    /// O quadro que vai para quem está entrando.
    #[must_use]
    pub fn anuncio(&self) -> ServerMessage {
        ServerMessage::ModsExigidos {
            mods: self.mods.clone(),
            conjunto: self.identidade.clone(),
        }
    }
}

/// Por que este servidor não consegue dizer o que exige.
///
/// Uma variante por causa, com o identificador dentro. `specs/02-protocolo.md`:
/// nenhuma string livre chega à interface — o `Display` daqui é para o log de
/// quem hospeda, que é quem pode consertar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NaoDaParaAnunciar {
    /// O banco não respondeu.
    #[error("a tabela de MODs não respondeu")]
    BancoNaoRespondeu,

    /// Uma linha habilitada tem um hash que não é um hash de conteúdo.
    ///
    /// O caso real que isto pega: o plano do núcleo local manda escrever a
    /// linha à mão com `hash` vazio, e escreve o porquê — «nada confere hash
    /// contra o que um servidor anunciou, porque nada atravessa o fio ainda. O
    /// plano que põe o anúncio no protocolo é o que fecha isso». É aqui que
    /// fecha.
    #[error("o MOD {id} está habilitado com um hash que não é um hash de conteúdo")]
    HashQueNaoEHash {
        /// Qual MOD.
        id: String,
    },

    /// O anúncio montado não passa na conferência do protocolo.
    ///
    /// Hoje isso quer dizer MODs demais para um quadro de controle, ou um campo
    /// acima do teto. A causa vai no log inteira.
    #[error("o anúncio não passou na conferência do protocolo: {erro}")]
    NaoPassaNoQuadro {
        /// O que o protocolo recusou.
        erro: String,
    },
}

/// Monta o conjunto que este servidor exige.
///
/// # Errors
///
/// [`NaoDaParaAnunciar`] quando a tabela não responde, quando uma linha
/// habilitada não descreve bytes conferíveis, ou quando o anúncio não cabe num
/// quadro de controle.
pub fn conjunto_exigido(persistence: &Persistence) -> Result<Conjunto, NaoDaParaAnunciar> {
    let ligados = crate::persistence::mods::enabled(persistence).map_err(|erro| {
        tracing::error!(%erro, "não deu para ler a tabela de MODs");
        NaoDaParaAnunciar::BancoNaoRespondeu
    })?;

    let mut mods: Vec<ModAnunciado> = Vec::with_capacity(ligados.len());
    for ligado in ligados {
        // **Antes de montar, e nomeando o MOD.** O protocolo recusaria o quadro
        // inteiro adiante, e a recusa dele diz «um hash está errado» sem dizer
        // de quem — que manda quem hospeda procurar numa lista.
        if !e_hash_de_conteudo(&ligado.hash) {
            return Err(NaoDaParaAnunciar::HashQueNaoEHash { id: ligado.id });
        }
        mods.push(ModAnunciado {
            id: ligado.id,
            version: ligado.version,
            hash: ligado.hash,
            repo: ligado.repo,
            reach: ligado.reach,
            no_servidor: ligado.server_half,
        });
    }

    // `identidade_do_conjunto` ordena a lista em quem a recebe, e é de propósito:
    // a ordem em que ela sai daqui é a ordem canônica, então o que vai no fio e
    // o que foi somado são a mesma coisa.
    let identidade = hex(&identidade_do_conjunto(&mut mods));
    let conjunto = Conjunto { mods, identidade };

    // A mesma conferência que `encode` faria adiante, feita aqui para que a
    // falha tenha nome antes de virar um quadro que não sai.
    if let Err(erro) = conjunto.anuncio().validate() {
        return Err(NaoDaParaAnunciar::NaoPassaNoQuadro {
            erro: erro.to_string(),
        });
    }
    Ok(conjunto)
}

/// Se o anúncio tem como alcançar **algum** par nesta build.
///
/// # O portão que não protege ninguém
///
/// `limiar` é a versão a partir da qual o anúncio sai — normalmente
/// [`seele_proto::mods::VERSAO_DO_ANUNCIO`]. O aperto de mão negocia o mínimo
/// entre o que o par declara e [`seele_proto::version::PROTOCOL_VERSION`], de
/// modo que **nenhuma conexão pode chegar acima da versão global**. Enquanto o
/// limiar estiver acima dela, portanto, não existe par no mundo capaz de ler o
/// anúncio nem de responder a ele.
///
/// Um portão que ninguém pode atravessar não é um guarda: é uma recusa de cem
/// por cento. Foi o que esta entrega produziu ao nascer com o limiar em 5 e a
/// versão global em 4 — habilitar um MOD, que até então era inócuo para quem
/// entra, passou a recusar toda entrada com `Incompatible` e a derrubar quem
/// já estava dentro, e nada em troca: ninguém ganhou a chance de aceitar.
///
/// Então enquanto o anúncio não alcança ninguém ele fica **dormente**: o
/// servidor admite como admitia antes daquela entrega e diz no log de quem
/// hospeda que a exigência ainda não vale no fio.
///
/// # E o dia chegou, em 14/09/2026
///
/// `PROTOCOL_VERSION` subiu para 5 na integração conjunta com a malha — a que
/// `VERSAO_DO_ANUNCIO` descreve —, alcançou o limiar padrão, e **o portão ligou
/// sozinho, sem uma linha a mais aqui dentro**. Esta função não mudou; a
/// comparação que ela sempre fez passou a dar verdadeiro.
///
/// Recusar um par abaixo do limiar volta a ser a coisa certa agora, e é o que
/// `session::exigir_aceite_dos_mods` continua fazendo: já existem builds que
/// aceitam, e uma que não alcança é incompatível de verdade. **A dormência
/// continua implementada de propósito** — ela não é entulho da transição: um
/// servidor com `versao_do_anuncio` acima da global, seja por configuração, seja
/// pela próxima variante que nascer adiantada, cai nela de novo e volta a
/// admitir em vez de recusar todo mundo.
#[must_use]
pub const fn o_anuncio_alcanca_alguem(limiar: u8) -> bool {
    limiar <= seele_proto::version::PROTOCOL_VERSION
}

/// Se habilitar um MOD hoje, nesta build, já impede quem não aceita de entrar
/// pela rede.
///
/// **Existe para a casca não fingir que sabe o que não sabe.** `enable`
/// (`crate::persistence::mods`) grava a exigência no banco e sempre teve
/// efeito — é o que a tela de quem hospeda mostra como ligado. O que essa tela
/// não podia dizer sem este nome é a outra metade: enquanto
/// [`o_anuncio_alcanca_alguem`] devolver falso para o limiar padrão, ligar um
/// MOD aqui não tranca ninguém na porta — é exatamente o «produto sabe e não
/// conta» que o `CLAUDE.md` deste repositório nomeia como o defeito mais caro,
/// e que só não virou um porque este nome existe para ser perguntado.
///
/// **Desde 14/09/2026 ela devolve verdadeiro**, com a subida de
/// `PROTOCOL_VERSION` para 5. A pergunta continua valendo a pena: a resposta é
/// de quem hospeda, não do código que a lê, e ela volta a ser falsa em qualquer
/// build cujo limiar padrão suba sem a versão global junto.
///
/// Usa [`seele_proto::mods::VERSAO_DO_ANUNCIO`] — o limiar padrão, o mesmo que
/// `ServerConfig::versao_do_anuncio` assume quando ninguém o sobrescreve, o que
/// vale para todo servidor hospedado por este app. Um limiar diferente, como o
/// que os testes de `session` usam para simular o dia em que o portão liga, não
/// é uma pergunta que a casca faz.
///
/// Ver `docs/pendencias.md` #39.
#[must_use]
pub const fn exigencia_vale_na_rede() -> bool {
    o_anuncio_alcanca_alguem(seele_proto::mods::VERSAO_DO_ANUNCIO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::mods::{disable, enable, EnabledMod};
    use crate::persistence::Location;

    /// O portão ligou, e é isto que fica escrito no lugar do estado anterior.
    ///
    /// A versão antiga deste teste dizia «hoje o anúncio ainda não alcança par
    /// nenhum» e existia para falhar no dia da integração conjunta — para que
    /// alguém conferisse de propósito que o portão ligou, em vez de descobrir
    /// pelo primeiro servidor que passou a recusar. Ele falhou, foi conferido, e
    /// agora prende a outra ponta.
    #[test]
    fn o_anuncio_alcanca_quem_negocia_a_versao_de_hoje() {
        assert!(
            o_anuncio_alcanca_alguem(seele_proto::mods::VERSAO_DO_ANUNCIO),
            "o anúncio voltou a ser dormente: habilitar um MOD deixou de trancar \
             quem não aceita, e a casca precisa voltar a dizer isso"
        );
        assert!(
            o_anuncio_alcanca_alguem(seele_proto::version::PROTOCOL_VERSION),
            "um limiar na versão global tem de alcançar quem negocia a versão global"
        );
    }

    /// A dormência não virou código morto, e esta é a prova.
    ///
    /// `existir não é funcionar`: um ramo que ninguém mais exercita é um ramo
    /// que apodrece em silêncio até a próxima variante nascer adiantada e
    /// precisar dele. Um limiar acima da versão global continua sem alcançar
    /// ninguém, e o servidor volta a admitir em vez de recusar cem por cento.
    #[test]
    fn um_limiar_acima_da_versao_global_continua_sem_alcancar_ninguem() {
        assert!(
            !o_anuncio_alcanca_alguem(seele_proto::version::PROTOCOL_VERSION + 1),
            "um limiar que nenhuma conexão pode negociar passou a contar como \
             alcançável, e um portão assim recusa todo mundo sem dar a ninguém a \
             chance de aceitar"
        );
    }

    /// A mesma pergunta que a casca faz, com o nome que ela usa para fazê-la —
    /// para o dia em que o portão ligar não passar só pelo teste acima, escrito
    /// contra o nome interno que a interface não chama.
    #[test]
    fn exigencia_vale_na_rede_concorda_com_o_limiar_padrao() {
        assert_eq!(
            exigencia_vale_na_rede(),
            o_anuncio_alcanca_alguem(seele_proto::mods::VERSAO_DO_ANUNCIO)
        );
        assert!(
            exigencia_vale_na_rede(),
            "a interface voltaria a ter de avisar que `enabled` ainda não tranca \
             ninguém na porta — confira o texto que `frases.js`/`base.js` mostram \
             ao lado do interruptor antes de mexer nesta linha"
        );
    }

    fn banco() -> Persistence {
        Persistence::open(&Location::Memory).expect("abrir memória")
    }

    fn linha(id: &str, hash_de: u8) -> EnabledMod {
        EnabledMod {
            id: id.to_owned(),
            version: "1.0.0".to_owned(),
            hash: format!("{hash_de:02x}").repeat(32),
            repo: "https://github.com/seele/exemplo".to_owned(),
            reach: vec!["dom".to_owned()],
            server_half: false,
        }
    }

    #[test]
    fn um_servidor_sem_mod_exige_um_conjunto_vazio() {
        let conjunto = conjunto_exigido(&banco()).expect("montar");
        assert!(conjunto.vazio());
    }

    /// O anúncio carrega identidade, versão e hash — e o que a tela de aceite
    /// precisa dizer antes de baixar qualquer byte.
    #[test]
    fn o_anuncio_carrega_identidade_versao_hash_e_o_que_a_tela_precisa() {
        let persistence = banco();
        let mut ligado = linha("seele/bot", 0xa1);
        ligado.reach = vec!["ler".to_owned(), "escrever".to_owned()];
        ligado.server_half = true;
        enable(&persistence, &ligado).expect("habilitar");

        let conjunto = conjunto_exigido(&persistence).expect("montar");
        assert_eq!(conjunto.mods.len(), 1);
        let anunciado = &conjunto.mods[0];
        assert_eq!(anunciado.id, "seele/bot");
        assert_eq!(anunciado.version, "1.0.0");
        assert_eq!(anunciado.hash, "a1".repeat(32));
        assert_eq!(anunciado.repo, "https://github.com/seele/exemplo");
        assert_eq!(anunciado.reach, vec!["ler", "escrever"]);
        assert!(
            anunciado.no_servidor,
            "a metade de servidor sumiu do anúncio, e é ela que diz que este MOD \
             roda na máquina de quem hospeda"
        );
        assert!(conjunto.anuncio().validate().is_ok());
    }

    /// **Instalado, habilitado e exigido são três coisas.** Desabilitar tira do
    /// conjunto sem apagar a linha nem os dados — ADR 0045 —, e o conjunto
    /// muda de identidade, que é o que faz o aceite anterior deixar de valer.
    #[test]
    fn desabilitar_tira_do_conjunto_e_muda_a_identidade() {
        let persistence = banco();
        enable(&persistence, &linha("seele/cor", 0xa1)).expect("um");
        enable(&persistence, &linha("seele/bot", 0xb2)).expect("dois");
        let com_dois = conjunto_exigido(&persistence).expect("montar");

        disable(&persistence, "seele/bot").expect("desabilitar");
        let com_um = conjunto_exigido(&persistence).expect("montar de novo");

        assert_eq!(com_um.mods.len(), 1);
        assert_ne!(
            com_dois.identidade, com_um.identidade,
            "a identidade não mudou quando o conjunto mudou; um aceite de antes \
             continuaria valendo"
        );
    }

    /// A ordem do banco não pode mudar a identidade, ou o aceite guardado
    /// deixaria de valer numa reconexão sem nada ter mudado.
    #[test]
    fn a_identidade_nao_depende_da_ordem_em_que_os_mods_foram_ligados() {
        let primeiro = banco();
        enable(&primeiro, &linha("seele/cor", 0xa1)).expect("um");
        enable(&primeiro, &linha("seele/bot", 0xb2)).expect("dois");

        let segundo = banco();
        enable(&segundo, &linha("seele/bot", 0xb2)).expect("dois");
        enable(&segundo, &linha("seele/cor", 0xa1)).expect("um");

        assert_eq!(
            conjunto_exigido(&primeiro).expect("montar").identidade,
            conjunto_exigido(&segundo).expect("montar").identidade
        );
    }

    /// O `hash` vazio que o plano do núcleo local manda escrever à mão para
    /// provar o runtime. Ele era deliberado enquanto nada atravessava o fio;
    /// agora ele é uma exigência que não se pode conferir, e o servidor diz
    /// qual linha é.
    #[test]
    fn uma_linha_habilitada_sem_hash_conferivel_e_nomeada() {
        let persistence = banco();
        let mut torta = linha("seele/primeiro", 0x00);
        torta.hash = String::new();
        enable(&persistence, &torta).expect("habilitar");

        assert_eq!(
            conjunto_exigido(&persistence),
            Err(NaoDaParaAnunciar::HashQueNaoEHash {
                id: "seele/primeiro".to_owned()
            })
        );
    }

    /// MODs demais não viram um anúncio cortado pela metade.
    #[test]
    fn mods_demais_para_um_quadro_nao_viram_um_anuncio_pela_metade() {
        let persistence = banco();
        for n in 0..=seele_proto::control::MAX_MODS_NO_ANUNCIO {
            enable(&persistence, &linha(&format!("seele/m{n}"), 0x11)).expect("habilitar");
        }
        assert!(matches!(
            conjunto_exigido(&persistence),
            Err(NaoDaParaAnunciar::NaoPassaNoQuadro { .. })
        ));
    }
}
