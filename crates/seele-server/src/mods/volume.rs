//! As esperas de volume: quem autorizou o quê, para quem, e até quando.
//!
//! ADR 0048. Os bytes de uma imagem deixam de atravessar o canal de controle e
//! passam por um fluxo próprio, direto ao disco — o QuickJS nunca os vê. Isso
//! abre uma pergunta que o caminho antigo não tinha: **contra o que o servidor
//! confere um fluxo que chega?**
//!
//! O token nasce dentro do MOD, e o servidor não interpreta o estado do MOD nem
//! deve. Se o cabeçalho do fluxo trouxesse só `{mod, token}`, «autorizar» seria
//! formalidade: bastaria abrir um fluxo com qualquer token inventado.
//!
//! Então o MOD **registra a espera** aqui, de dentro do `aoPedir`, no mesmo ato
//! em que responde à janela. É contra ela que o cabeçalho é conferido.
//!
//! # Três propriedades, e cada uma fecha um buraco
//!
//! - **A pessoa vem do servidor, não do MOD.** `esperar` não recebe quem é: ele
//!   lê de quem o `Hospedes::pedir` disse estar atendendo. Um MOD com defeito
//!   não tem como prender um token à pessoa errada, e um MOD mal-intencionado
//!   não tem como prendê-lo a alguém que não pediu nada.
//! - **Vale uma vez.** [`Esperas::tomar`] remove. Sem isso, um token vazado
//!   viraria um lugar de escrita permanente na pasta do MOD, para quem o
//!   tivesse.
//! - **Tem prazo.** Uma espera que ninguém consome não fica de pé para sempre
//!   ocupando memória e esperando alguém que desistiu.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use seele_proto::ids::PersonId;

/// A pasta, dentro dos dados de um MOD, onde o volume mora.
///
/// **Nomeada à parte de propósito** — ADR 0048. Um arquivo de volume não é um
/// arquivo de MOD comum: `arquivos::escrever` tem teto de 4 MiB por arquivo, e
/// o volume não tem teto nenhum. Misturá-los apagaria aquele teto para todo o
/// resto, que é o que protege o disco de quem hospeda hoje.
pub const PASTA: &str = "volume";

/// Quanto entra e sai do disco de uma vez.
///
/// O mesmo de `transfer.rs`, e pela mesma razão escrita lá: o servidor é
/// dimensionado em 1 vCPU e 512 MB, e nada segura o arquivo inteiro.
pub const BLOCO: usize = 64 * 1024;

/// O maior prazo que uma espera pode pedir.
///
/// Dez minutos é o que o envio em fragmentos já usava, e sobra: por fluxo, uma
/// imagem de 10 MiB passa dentro da rajada de bytes. O teto existe para que um
/// MOD não peça um prazo de um dia e deixe esperas de pé até o servidor
/// reiniciar.
pub const PRAZO_MAXIMO: Duration = Duration::from_secs(600);

/// **O teto de bytes de um upload de volume**, valendo para todo fluxo.
///
/// O ADR 0048 decidiu que o volume não tem teto *de disco* (a soma de tudo que
/// um MOD guarda), e isso continua: o teto é **por envio**. Sem ele, quem tem
/// uma única espera registrada transmitia sem parar para o `.em-obras-*` — o
/// tipo só era lido depois do último byte. É o mesmo valor que o caminho de
/// imagem por fluxo já impõe (`PedidoDeImagem::Enviar` recusa `bytes` acima de
/// `TETO_DE_ARQUIVO`) e que a leitura (`Ler`) recusa ao servir: uma imagem que
/// o produto não serve de volta não faz sentido o produto aceitar.
///
/// **É o maior dos tetos por tipo** (a fusão da 0.19 com a API 8): 100 MiB
/// de imagem desde a 0.19, 64 MiB de `.mrpack` (API 8 parte 6). O de cada
/// tipo vale assim que o tipo é lido, no começo do fluxo — ver
/// [`teto_da_espera`] e [`Formato::teto`].
///
/// A [`Espera`] não declara tamanho máximo (isso mudaria a API publicada do
/// MOD); quando o envio declara o total, vale o menor dos dois.
pub const TETO_FIXO: u64 = seele_proto::volume::TETO_DO_FIO_DE_VOLUME;

/// Quantas esperas de pé ao mesmo tempo, no servidor inteiro.
///
/// **Isto não é o teto de disco**, que o ADR 0048 decidiu não existir. É o teto
/// da *lista*: uma espera custa memória antes de qualquer byte chegar, e um MOD
/// num laço registrando esperas encheria a memória de quem hospeda sem nunca
/// enviar nada. O disco é decisão de quem hospeda; a memória do processo não.
pub const ESPERAS_DE_PE: usize = 256;

/// Uma autorização de escrita, registrada pelo MOD e conferida pelo servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Espera {
    /// Qual MOD autorizou. O fluxo tem de nomear o mesmo.
    pub mod_id: String,
    /// Para quem. Vem do servidor, e não do MOD.
    pub pessoa: PersonId,
    /// Onde gravar, dentro da pasta daquele MOD. Passa pelo `inner_path` de
    /// sempre na hora de resolver — aqui é guardado como o MOD o escreveu.
    pub caminho: String,
    /// Os tipos de imagem que este MOD aceita neste envio.
    ///
    /// É o servidor quem confere os bytes (ADR 0048), e é esta lista que diz
    /// contra o quê. Vazia significa «nenhum», e não «todos»: um MOD que
    /// esquecer de declarar não passa a aceitar qualquer coisa.
    pub tipos: Vec<String>,
    /// Até quando ela vale.
    pub ate: Instant,
}

/// As esperas de pé.
#[derive(Debug, Default)]
pub struct Esperas {
    por_token: HashMap<String, Espera>,
}

/// Por que uma espera não foi registrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaoEsperou {
    /// Chamado fora de um pedido: não há quem atender.
    SemPedidoEmCurso,
    /// Token vazio, caminho vazio, ou nenhum tipo declarado.
    PedidoIncompleto,
    /// Já há uma espera com este token.
    TokenRepetido,
    /// A lista está cheia. Ver [`ESPERAS_DE_PE`].
    ListaCheia,
}

/// O que o MOD pede ao registrar uma espera.
///
/// Uma estrutura e não sete argumentos: os campos são todos texto e número, e
/// uma assinatura assim é uma assinatura em que trocar dois de lugar compila.
pub struct PedidoDeEspera {
    /// Qual MOD autoriza. Posto pelo servidor, não pelo MOD.
    pub mod_id: String,
    /// Para quem. Posto pelo servidor, não pelo MOD.
    pub pessoa: PersonId,
    /// O token que o fluxo vai apresentar.
    pub token: String,
    /// Onde gravar, dentro da pasta daquele MOD.
    pub caminho: String,
    /// Os tipos de imagem aceitos. Vazia é «nenhum», e não «todos».
    pub tipos: Vec<String>,
    /// Quanto tempo a espera vale, aparado em [`PRAZO_MAXIMO`].
    pub prazo: Duration,
}

impl Esperas {
    /// Regista uma espera, ou diz por que não.
    ///
    /// O prazo é aparado em [`PRAZO_MAXIMO`]; um MOD que peça mais recebe o
    /// máximo em vez de uma recusa, porque pedir demais é engano comum e
    /// recusar por isso não protege ninguém.
    ///
    /// # Errors
    ///
    /// [`NaoEsperou`] quando falta o que identifica a espera, quando o token
    /// repete, ou quando a lista está cheia.
    pub fn registrar(&mut self, pedido: PedidoDeEspera, agora: Instant) -> Result<(), NaoEsperou> {
        if pedido.token.is_empty() || pedido.caminho.is_empty() || pedido.tipos.is_empty() {
            return Err(NaoEsperou::PedidoIncompleto);
        }
        self.podar(agora);
        if self.por_token.contains_key(&pedido.token) {
            return Err(NaoEsperou::TokenRepetido);
        }
        if self.por_token.len() >= ESPERAS_DE_PE {
            return Err(NaoEsperou::ListaCheia);
        }
        self.por_token.insert(
            pedido.token,
            Espera {
                mod_id: pedido.mod_id,
                pessoa: pedido.pessoa,
                caminho: pedido.caminho,
                tipos: pedido.tipos,
                ate: agora + pedido.prazo.min(PRAZO_MAXIMO),
            },
        );
        Ok(())
    }

    /// Consome a espera deste token, se ela é desta pessoa, deste MOD, e ainda
    /// vale.
    ///
    /// **Remove.** Uma espera consumida não existe mais: é o que impede um
    /// token vazado de virar escrita repetida.
    pub fn tomar(
        &mut self,
        token: &str,
        mod_id: &str,
        pessoa: PersonId,
        agora: Instant,
    ) -> Option<Espera> {
        let espera = self.por_token.get(token)?;
        // Conferir **antes** de remover: um token certo com a pessoa errada não
        // pode consumir a espera de quem a registrou. Seria transformar um
        // palpite numa negação de serviço.
        if espera.mod_id != mod_id || espera.pessoa != pessoa || espera.ate <= agora {
            return None;
        }
        self.por_token.remove(token)
    }

    /// Esquece o que venceu.
    pub fn podar(&mut self, agora: Instant) {
        self.por_token.retain(|_, e| e.ate > agora);
    }

    /// Quantas estão de pé. Para o teste e para quem hospeda olhar.
    #[must_use]
    pub fn quantas(&self) -> usize {
        self.por_token.len()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// **A tabela dos nomes que o MOD lê** (a auditoria do `v6.json`, B1):
    /// cada recusa de volume, com o que os bytes são, vira um nome da lista
    /// `recusas.volume`. O teste de ponta a ponta, em `ponte_de_pedidos.rs`,
    /// alcança três; este cobre a tabela inteira.
    #[test]
    fn cada_recusa_de_volume_vira_um_nome_da_lista() {
        use seele_proto::volume::VolumeRefusal;
        let casos = [
            (Recusa::from(VolumeRefusal::SemEspera), "sem-espera"),
            (Recusa::from(VolumeRefusal::NaoGravei), "nao-gravei"),
            (Recusa::from(VolumeRefusal::Incompleto), "incompleto"),
            (Recusa::from(VolumeRefusal::TipoRecusado), "tipo-recusado"),
            (
                Recusa {
                    motivo: VolumeRefusal::TipoRecusado,
                    detectado: Detectado::Tipo("gif"),
                },
                "tipo-recusado:gif",
            ),
            (
                Recusa {
                    motivo: VolumeRefusal::TipoRecusado,
                    detectado: Detectado::FormatoNenhum,
                },
                "formato-desconhecido",
            ),
        ];
        for (recusa, nome) in casos {
            assert_eq!(recusa.nome_para_o_mod(), nome, "{recusa:?}");
        }
    }

    fn pessoa(n: u64) -> PersonId {
        PersonId(n)
    }

    fn pedido(token: &str, quem: PersonId, tipos: Vec<String>, prazo: u64) -> PedidoDeEspera {
        PedidoDeEspera {
            mod_id: "seele/perfis".to_owned(),
            pessoa: quem,
            token: token.to_owned(),
            caminho: "volume/avatar.bin".to_owned(),
            tipos,
            prazo: Duration::from_secs(prazo),
        }
    }

    fn registrada(esperas: &mut Esperas, token: &str, quem: PersonId, agora: Instant) {
        esperas
            .registrar(pedido(token, quem, vec!["png".to_owned()], 60), agora)
            .expect("registrar");
    }

    #[test]
    fn a_espera_registrada_e_consumida_uma_vez_so() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        registrada(&mut esperas, "t1", pessoa(7), agora);

        assert!(esperas
            .tomar("t1", "seele/perfis", pessoa(7), agora)
            .is_some());
        assert!(
            esperas
                .tomar("t1", "seele/perfis", pessoa(7), agora)
                .is_none(),
            "um token vazado viraria um lugar de escrita permanente na pasta do MOD"
        );
    }

    #[test]
    fn a_espera_e_de_uma_pessoa_e_de_um_mod() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        registrada(&mut esperas, "t1", pessoa(7), agora);

        assert!(
            esperas
                .tomar("t1", "seele/perfis", pessoa(8), agora)
                .is_none(),
            "outra pessoa consumiu a autorização de quem a pediu"
        );
        assert!(
            esperas
                .tomar("t1", "seele/estilo", pessoa(7), agora)
                .is_none(),
            "outro MOD escreveu na pasta usando token alheio"
        );
        assert_eq!(
            esperas.quantas(),
            1,
            "a tentativa errada consumiu a espera de quem tinha direito a ela: \
             adivinhar um token viraria negação de serviço"
        );
        assert!(esperas
            .tomar("t1", "seele/perfis", pessoa(7), agora)
            .is_some());
    }

    #[test]
    fn uma_espera_vencida_nao_vale_e_nao_fica_de_pe() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        registrada(&mut esperas, "t1", pessoa(7), agora);

        let depois = agora + Duration::from_secs(61);
        assert!(esperas
            .tomar("t1", "seele/perfis", pessoa(7), depois)
            .is_none());
        esperas.podar(depois);
        assert_eq!(esperas.quantas(), 0);
    }

    /// Um MOD que peça um dia recebe o máximo, e não uma recusa: pedir demais é
    /// engano comum, e recusar por isso não protege ninguém.
    #[test]
    fn um_prazo_alem_do_teto_e_aparado_e_nao_recusado() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        esperas
            .registrar(
                pedido("t1", pessoa(7), vec!["png".to_owned()], 86_400),
                agora,
            )
            .expect("registrar");

        let quase = agora + PRAZO_MAXIMO - Duration::from_secs(1);
        assert!(esperas
            .tomar("t1", "seele/perfis", pessoa(7), quase)
            .is_some());
    }

    /// Vazia é **nenhum**, e não «todos»: um MOD que esquecer de declarar os
    /// tipos não passa a aceitar qualquer coisa.
    #[test]
    fn uma_espera_sem_tipo_declarado_nao_e_registrada() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        assert_eq!(
            esperas.registrar(pedido("t1", pessoa(7), Vec::new(), 60), agora),
            Err(NaoEsperou::PedidoIncompleto)
        );
        assert_eq!(esperas.quantas(), 0);
    }

    /// O teto é da **lista**, e não do disco: uma espera custa memória antes de
    /// um byte chegar, e um MOD num laço encheria a memória de quem hospeda.
    #[test]
    fn um_laco_de_esperas_nao_enche_a_memoria_de_quem_hospeda() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        for n in 0..ESPERAS_DE_PE {
            registrada(&mut esperas, &format!("t{n}"), pessoa(7), agora);
        }
        assert_eq!(
            esperas.registrar(
                pedido("demais", pessoa(7), vec!["png".to_owned()], 60),
                agora
            ),
            Err(NaoEsperou::ListaCheia)
        );
        assert_eq!(esperas.quantas(), ESPERAS_DE_PE);
    }

    /// E a lista cheia de esperas **vencidas** não tranca o servidor: a poda
    /// acontece antes de recusar, senão um pico de dez minutos atrás impediria
    /// todo envio até alguém reiniciar.
    #[test]
    fn a_lista_cheia_de_vencidas_volta_a_aceitar_sozinha() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        for n in 0..ESPERAS_DE_PE {
            registrada(&mut esperas, &format!("t{n}"), pessoa(7), agora);
        }
        let depois = agora + Duration::from_secs(61);
        esperas
            .registrar(
                pedido("nova", pessoa(7), vec!["png".to_owned()], 60),
                depois,
            )
            .expect("a poda tem de abrir espaço");
        assert_eq!(esperas.quantas(), 1);
    }

    #[test]
    fn o_mesmo_token_duas_vezes_e_recusado() {
        let agora = Instant::now();
        let mut esperas = Esperas::default();
        registrada(&mut esperas, "t1", pessoa(7), agora);
        assert_eq!(
            esperas.registrar(pedido("t1", pessoa(7), vec!["png".to_owned()], 60), agora),
            Err(NaoEsperou::TokenRepetido)
        );
    }

    // ---- o teto e o tipo valem durante o fluxo (volume sem teto de disco)

    /// Um fluxo que entrega `inicio` e depois `zeros` bytes de zero, em blocos
    /// de [`BLOCO`], contando quanto foi lido.
    struct Contada {
        inicio: Vec<u8>,
        zeros: u64,
        lidos: u64,
    }

    impl Fonte for Contada {
        async fn ler(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ()> {
            let n = if self.inicio.is_empty() {
                let n = usize::try_from(self.zeros.min(buf.len() as u64)).unwrap();
                buf.get_mut(..n).unwrap().fill(0);
                self.zeros -= n as u64;
                n
            } else {
                let n = self.inicio.len().min(buf.len());
                buf.get_mut(..n)
                    .unwrap()
                    .copy_from_slice(self.inicio.get(..n).unwrap());
                self.inicio.drain(..n);
                n
            };
            if n == 0 {
                return Ok(None);
            }
            self.lidos += n as u64;
            Ok(Some(n))
        }
    }

    const PNG: [u8; 12] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    /// Tudo que sobrou em `volume/`, de qualquer nome.
    fn sobras(raiz: &std::path::Path) -> Vec<String> {
        let mut achados = Vec::new();
        let mut pilha = vec![raiz.to_path_buf()];
        while let Some(pasta) = pilha.pop() {
            let Ok(entradas) = std::fs::read_dir(&pasta) else {
                continue;
            };
            for e in entradas.flatten() {
                if e.path().is_dir() {
                    pilha.push(e.path());
                } else {
                    achados.push(e.path().display().to_string());
                }
            }
        }
        achados
    }

    /// Roda um fluxo por `gravar_fluxo` com uma espera de `tipos`.
    async fn gravar(
        fonte: &mut Contada,
        tipos: &[&str],
        tamanho: Option<u64>,
    ) -> (Result<Chegou, (String, Recusa)>, tempfile::TempDir) {
        let temp = tempfile::tempdir().unwrap();
        let raizes = crate::RaizesDosMods {
            pacotes: temp.path().join("cache"),
            dados: temp.path().join("dados"),
        };
        let esperas = std::sync::Mutex::new(Esperas::default());
        let agora = Instant::now();
        esperas
            .lock()
            .unwrap()
            .registrar(
                PedidoDeEspera {
                    mod_id: "a/b".to_owned(),
                    pessoa: pessoa(7),
                    token: "t1".to_owned(),
                    caminho: "fotos/um.png".to_owned(),
                    tipos: tipos.iter().map(|t| (*t).to_owned()).collect(),
                    prazo: Duration::from_secs(60),
                },
                agora,
            )
            .unwrap();
        let cabecalho = seele_proto::volume::VolumeHeader {
            mod_id: "a/b".to_owned(),
            token: "t1".to_owned(),
        };
        let r = gravar_fluxo(
            &esperas,
            Some(&raizes),
            pessoa(7),
            fonte,
            cabecalho,
            tamanho,
        )
        .await;
        (r, temp)
    }

    #[tokio::test]
    async fn um_fluxo_sem_tamanho_declarado_para_no_teto_e_nao_deixa_arquivo() {
        // Um PNG de verdade na frente (passa no sniff), e depois muito mais que o teto.
        let mut fonte = Contada {
            inicio: PNG.to_vec(),
            zeros: TETO_FIXO + 8 * BLOCO as u64,
            lidos: 0,
        };
        let (r, temp) = gravar(&mut fonte, &["png"], None).await;
        let (_, recusa) = r.expect_err("passou do teto e foi aceito");
        assert_eq!(
            recusa.motivo,
            seele_proto::volume::VolumeRefusal::Incompleto
        );
        // Parou no bloco que cruzou o teto, e não leu o resto.
        assert!(
            fonte.lidos <= TETO_FIXO + BLOCO as u64,
            "leu {} bytes depois do teto",
            fonte.lidos
        );
        assert_eq!(sobras(temp.path()), Vec::<String>::new(), "sobrou arquivo");
    }

    #[tokio::test]
    async fn um_fluxo_que_nao_e_imagem_morre_nos_primeiros_bytes() {
        let mut fonte = Contada {
            inicio: b"MZ\x90\x00 isto nao e imagem".to_vec(),
            zeros: TETO_FIXO,
            lidos: 0,
        };
        let (r, temp) = gravar(&mut fonte, &["png"], None).await;
        let (_, recusa) = r.expect_err("um executável foi aceito como imagem");
        assert_eq!(
            recusa.motivo,
            seele_proto::volume::VolumeRefusal::TipoRecusado
        );
        assert_eq!(recusa.detectado, Detectado::FormatoNenhum);
        assert!(
            fonte.lidos <= BLOCO as u64,
            "leu {} bytes antes de recusar o tipo",
            fonte.lidos
        );
        assert_eq!(sobras(temp.path()), Vec::<String>::new());
    }

    #[tokio::test]
    async fn um_tipo_conhecido_mas_nao_autorizado_morre_nos_primeiros_bytes() {
        let mut fonte = Contada {
            inicio: PNG.to_vec(),
            zeros: TETO_FIXO,
            lidos: 0,
        };
        let (r, _temp) = gravar(&mut fonte, &["gif"], None).await;
        let (_, recusa) = r.expect_err("png aceito onde só gif foi autorizado");
        assert_eq!(recusa.detectado, Detectado::Tipo("png"));
        assert!(fonte.lidos <= BLOCO as u64, "leu {}", fonte.lidos);
    }

    // ---- os tetos por tipo, na fusão da API 8 com a 0.19

    const ZIP: [u8; 12] = [0x50, 0x4b, 0x03, 0x04, 0, 0, 0, 0, 0, 0, 0, 0];

    /// **Um `.mrpack` declarado acima de 64 MiB morre no sniff**, mesmo numa
    /// espera que também aceita imagem (cujo teto é 100 MiB): lido o começo de
    /// zip, o teto é o do pack, e nada vai ao disco.
    #[tokio::test]
    async fn o_pack_declarado_acima_do_teto_dele_morre_no_sniff() {
        let total = 80 * 1024 * 1024;
        let mut fonte = Contada {
            inicio: ZIP.to_vec(),
            zeros: total - ZIP.len() as u64,
            lidos: 0,
        };
        let (r, temp) = gravar(&mut fonte, &["png", "mrpack"], Some(total)).await;
        let (_, recusa) = r.expect_err("um pack de 80 MiB foi aceito");
        assert_eq!(
            recusa.detectado,
            Detectado::AcimaDoTeto {
                bytes: total,
                teto: seele_proto::pack::TETO_DO_PACK
            }
        );
        assert!(fonte.lidos <= BLOCO as u64, "leu {} bytes", fonte.lidos);
        assert_eq!(sobras(temp.path()), Vec::<String>::new());
    }

    /// **A imagem vai até 100 MiB** (a 0.19), e não até os 64 do pack nem os
    /// 10 de antes: uma de 70 MiB chega inteira numa espera que aceita os dois.
    #[tokio::test]
    async fn uma_imagem_acima_do_teto_do_pack_chega_numa_espera_dos_dois() {
        let mut fonte = Contada {
            inicio: PNG.to_vec(),
            zeros: 70 * 1024 * 1024,
            lidos: 0,
        };
        let (r, _temp) = gravar(&mut fonte, &["png", "mrpack"], None).await;
        let chegou = r.expect("uma imagem de 70 MiB foi recusada");
        assert_eq!(chegou.bytes, 12 + 70 * 1024 * 1024);
        assert_eq!(chegou.tipo, "image/png");
    }

    /// Um zip sem o índice numa espera de `mrpack`: começa como pack, e é
    /// recusado no fim, como formato desconhecido, sem sobra.
    #[tokio::test]
    async fn um_zip_que_nao_e_pack_e_recusado_no_fim() {
        let mut fonte = Contada {
            inicio: ZIP.to_vec(),
            zeros: 3 * BLOCO as u64,
            lidos: 0,
        };
        let (r, temp) = gravar(&mut fonte, &["mrpack"], None).await;
        let (_, recusa) = r.expect_err("um zip sem índice virou pack");
        assert_eq!(recusa.detectado, Detectado::FormatoNenhum);
        assert_eq!(sobras(temp.path()), Vec::<String>::new());
    }

    /// Um zip numa espera só de imagem morre nos primeiros bytes, como antes.
    #[tokio::test]
    async fn um_zip_numa_espera_de_imagem_morre_no_sniff() {
        let mut fonte = Contada {
            inicio: ZIP.to_vec(),
            zeros: TETO_FIXO,
            lidos: 0,
        };
        let (r, _temp) = gravar(&mut fonte, &["png"], None).await;
        let (_, recusa) = r.expect_err("um zip entrou numa espera de png");
        assert_eq!(recusa.detectado, Detectado::FormatoNenhum);
        assert!(fonte.lidos <= BLOCO as u64, "leu {}", fonte.lidos);
    }

    #[tokio::test]
    async fn uma_imagem_dentro_do_teto_continua_chegando_inteira() {
        let mut fonte = Contada {
            inicio: PNG.to_vec(),
            zeros: 3 * BLOCO as u64,
            lidos: 0,
        };
        let (r, temp) = gravar(&mut fonte, &["png"], None).await;
        let chegou = r.expect("recusou uma imagem válida");
        assert_eq!(chegou.bytes, 12 + 3 * BLOCO as u64);
        let gravado = std::fs::read(temp.path().join("dados/a/b/volume/fotos/um.png")).unwrap();
        assert_eq!(gravado.len() as u64, chegou.bytes);
        assert_eq!(gravado.get(..12), Some(&PNG[..]));
    }
}

// ---------------------------------------------------------------- o recebimento

/// O que um fluxo de volume deixou no disco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chegou {
    /// De qual MOD é a pasta.
    pub mod_id: String,
    /// Para quem a espera estava registrada.
    pub pessoa: PersonId,
    /// A autorização consumida. Vai de volta ao MOD no veredito.
    pub token: String,
    /// Onde o arquivo ficou, como o MOD o nomeou.
    pub caminho: String,
    /// Quantos bytes chegaram.
    pub bytes: u64,
    /// O SHA-256 do conteúdo, em hexadecimal minúsculo.
    pub hash: String,
    /// O tipo que os **bytes** dizem ser — nunca o que alguém declarou.
    pub tipo: &'static str,
}

/// Recebe um fluxo de volume, do cabeçalho ao arquivo em disco.
///
/// # A ordem das conferências, e por que ela é esta
///
/// 1. **O cabeçalho cabe no fio.** Antes de qualquer alocação.
/// 2. **A espera existe, é deste MOD, desta pessoa, e ainda vale.** Consumida
///    aqui, e por isso um token vazado não vira escrita repetida.
/// 3. **O caminho resolve dentro da pasta.** Pelo `dentro` de sempre, e sob
///    [`PASTA`]: um caminho que sobe morre aqui, mesmo tendo vindo do MOD.
/// 4. **Os bytes vão para um arquivo de obras**, em blocos, com o hash sendo
///    somado no caminho. Nada segura o arquivo inteiro.
/// 5. **O tipo é lido dos bytes** e conferido contra o que o MOD declarou
///    aceitar. Só então o arquivo de obras vira o arquivo final.
///
/// O quinto passo é o que torna «sem teto» uma decisão e não um descuido do
/// outro lado: o arquivo só aparece no lugar que o MOD nomeou depois de o
/// produto ter olhado os bytes.
///
/// # Errors
///
/// Devolve [`seele_proto::volume::VolumeRefusal`] — uma recusa é algo que quem
/// enviou está esperando ouvir, e não um erro de quem recebe.
pub async fn receber(
    esperas: &std::sync::Mutex<Esperas>,
    raizes: Option<&crate::RaizesDosMods>,
    pessoa: PersonId,
    fluxo: &mut crate::transporte::Recebe,
) -> Result<Chegou, (String, seele_proto::volume::VolumeRefusal)> {
    use seele_proto::volume::VolumeRefusal;

    let cabecalho: seele_proto::volume::VolumeHeader = crate::frame::read(fluxo)
        .await
        .map_err(|_| (String::new(), VolumeRefusal::Incompleto))?;
    receber_imagem(esperas, raizes, pessoa, fluxo, cabecalho, None)
        .await
        .map_err(|(token, recusa)| (token, recusa.motivo))
}

/// **Uma recusa de volume, com o que o MOD lê dela** (o corte da API 6; a
/// auditoria do `v6.json`, `recusas.volume`).
///
/// O fio do controle leva só o [`seele_proto::volume::VolumeRefusal`]
/// (`VolumeRecusado`), e ele não muda. O fluxo de imagem leva um texto, que a
/// casca entrega ao MOD em `e.erro` (`recusaParaOMod`), e esse texto tem de ser
/// um nome da lista fechada: era «imagem recusada: SemEspera», o `Debug` do
/// enum. E o que só os bytes dizem — o tipo que eles têm, ou que não têm tipo
/// nenhum — viaja até aqui, para o MOD saber se mandou um GIF onde pediu PNG
/// ou um arquivo que não é imagem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recusa {
    /// O que o controle diz.
    pub motivo: seele_proto::volume::VolumeRefusal,
    /// O que os bytes são, quando foi por eles que a recusa veio.
    pub detectado: Detectado,
}

/// O que os bytes de um volume recusado são.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detectado {
    /// A recusa não olhou os bytes.
    Nada,
    /// Um formato que o produto conhece e o MOD não autorizou: o sufixo dele.
    Tipo(&'static str),
    /// Bytes de formato nenhum.
    FormatoNenhum,
    /// **Maior que o teto do tipo** (API 8 parte 6, com o teto de imagem da
    /// 0.19): 100 MiB de imagem, 64 MiB de `.mrpack`. Conferido antes do
    /// primeiro byte quando o tamanho vem no pedido, e no corte quando não vem.
    AcimaDoTeto {
        /// O tamanho do que chegava.
        bytes: u64,
        /// O teto.
        teto: u64,
    },
}

impl From<seele_proto::volume::VolumeRefusal> for Recusa {
    fn from(motivo: seele_proto::volume::VolumeRefusal) -> Self {
        Self {
            motivo,
            detectado: Detectado::Nada,
        }
    }
}

impl Recusa {
    /// O nome que o MOD lê em `e.erro`, da lista `recusas.volume` do
    /// `api/v6.json`; o sufixo depois de `:` a casca faz campo (`detectado`).
    #[must_use]
    pub fn nome_para_o_mod(self) -> String {
        use seele_proto::volume::VolumeRefusal;
        match (self.motivo, self.detectado) {
            // O tamanho e o teto vão no nome, e a casca os faz campos
            // (`tamanho`, `teto`): a tela do MOD diz os dois, e sugere o
            // link do Modrinth para o pack.
            (_, Detectado::AcimaDoTeto { bytes, teto }) => {
                format!("arquivo-grande-demais:{bytes}:{teto}")
            }
            (VolumeRefusal::SemEspera, _) => "sem-espera".to_owned(),
            (VolumeRefusal::TipoRecusado, Detectado::Tipo(tipo)) => format!("tipo-recusado:{tipo}"),
            (VolumeRefusal::TipoRecusado, Detectado::FormatoNenhum) => {
                "formato-desconhecido".to_owned()
            }
            (VolumeRefusal::TipoRecusado, Detectado::Nada) => "tipo-recusado".to_owned(),
            (VolumeRefusal::NaoGravei, _) => "nao-gravei".to_owned(),
            (VolumeRefusal::Incompleto, _) => "incompleto".to_owned(),
        }
    }
}

/// Recebe o corpo com autorização consumível e tamanho opcional conferido.
pub async fn receber_imagem(
    esperas: &std::sync::Mutex<Esperas>,
    raizes: Option<&crate::RaizesDosMods>,
    pessoa: PersonId,
    fluxo: &mut crate::transporte::Recebe,
    cabecalho: seele_proto::volume::VolumeHeader,
    tamanho: Option<u64>,
) -> Result<Chegou, (String, Recusa)> {
    gravar_fluxo(esperas, raizes, pessoa, fluxo, cabecalho, tamanho).await
}

/// O corpo de [`receber_imagem`], sobre qualquer [`Fonte`]: é por aqui que os
/// testes passam um fluxo contado, sem abrir uma conexão.
async fn gravar_fluxo(
    esperas: &std::sync::Mutex<Esperas>,
    raizes: Option<&crate::RaizesDosMods>,
    pessoa: PersonId,
    fluxo: &mut impl Fonte,
    cabecalho: seele_proto::volume::VolumeHeader,
    tamanho: Option<u64>,
) -> Result<Chegou, (String, Recusa)> {
    use seele_proto::volume::VolumeRefusal;
    {
        use seele_proto::control::Validate as _;
        if cabecalho.validate().is_err() {
            return Err((String::new(), VolumeRefusal::SemEspera.into()));
        }
    }
    let token = cabecalho.token.clone();
    let errar = |motivo: VolumeRefusal| (token.clone(), Recusa::from(motivo));

    let espera = {
        let Ok(mut guarda) = esperas.lock() else {
            return Err(errar(VolumeRefusal::NaoGravei));
        };
        guarda
            .tomar(&cabecalho.token, &cabecalho.mod_id, pessoa, Instant::now())
            .ok_or_else(|| errar(VolumeRefusal::SemEspera))?
    };

    // Sem raiz não há onde gravar: este servidor é o de memória, e a recusa é
    // do disco e não do token — quem enviou precisa saber a diferença.
    // **O teto é o do tipo que a espera aceita** (API 8 parte 6), e é
    // conferido antes do primeiro byte quando o pedido diz o tamanho.
    let teto = teto_da_espera(&espera);
    if let Some(bytes) = tamanho.filter(|b| *b > teto) {
        return Err((
            token.clone(),
            Recusa {
                motivo: VolumeRefusal::Incompleto,
                detectado: Detectado::AcimaDoTeto { bytes, teto },
            },
        ));
    }
    let Some(raizes) = raizes else {
        return Err(errar(VolumeRefusal::NaoGravei));
    };
    // **Da instância, e não da máquina.** Ver `RaizesDosMods`: o mesmo MOD em
    // dois servidores desta máquina grava em lugares diferentes.
    let raiz = raizes.dados_de(&espera.mod_id).join(PASTA);
    let Some(destino) = crate::mods::arquivos::dentro(&raiz, &espera.caminho) else {
        return Err(errar(VolumeRefusal::SemEspera));
    };
    let Some(pai) = destino.parent() else {
        return Err(errar(VolumeRefusal::NaoGravei));
    };
    if tokio::fs::create_dir_all(pai).await.is_err() {
        return Err(errar(VolumeRefusal::NaoGravei));
    }

    // **Arquivo de obras, e não o destino.** Um fluxo que cai no meio deixaria
    // meia imagem no lugar onde o MOD espera uma imagem inteira — e o MOD não
    // tem como saber disso sem abrir os bytes, que é exatamente o que este
    // caminho existe para ele não precisar fazer.
    let mut soma_token = seele_proto::attachment::ContentDigest::new();
    soma_token.feed(cabecalho.token.as_bytes());
    let obras = pai.join(format!(".em-obras-{}", hex(&soma_token.finish())));
    let _limpeza = LimparObras(obras.clone());
    let resultado = escorrer(fluxo, &obras, &espera, tamanho).await;
    match resultado {
        Ok(chegou) => {
            if tokio::fs::rename(&obras, &destino).await.is_err() {
                let _ = tokio::fs::remove_file(&obras).await;
                return Err(errar(VolumeRefusal::NaoGravei));
            }
            Ok(Chegou {
                mod_id: espera.mod_id,
                pessoa: espera.pessoa,
                token: cabecalho.token,
                caminho: espera.caminho,
                ..chegou
            })
        }
        Err(motivo) => {
            // O de obras sai em toda saída que não é a de cima. Deixá-lo seria
            // lixo com nome de token na pasta de quem hospeda.
            let _ = tokio::fs::remove_file(&obras).await;
            Err((token.clone(), motivo))
        }
    }
}

// Também executa quando o timeout cancela a future no meio da escrita.
struct LimparObras(std::path::PathBuf);
impl Drop for LimparObras {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// De onde os bytes de um fluxo vêm: o [`crate::transporte::Recebe`] em
/// produção, e uma fonte contada nos testes, que precisam saber **quanto foi
/// lido** antes de a recusa sair.
trait Fonte {
    /// Lê até `buf.len()` bytes. `None` é o fim do fluxo.
    async fn ler(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ()>;
}

impl Fonte for crate::transporte::Recebe {
    async fn ler(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ()> {
        self.read(buf).await.map_err(|_| ())
    }
}

/// O que os primeiros bytes de um envio são.
#[derive(Debug, Clone, Copy)]
enum Formato {
    /// Uma das quatro imagens.
    Imagem(seele_proto::imagem::ImageFormat),
    /// **Um candidato a `.mrpack`** (API 8 parte 6): começa como um zip, e a
    /// espera aceita `mrpack`. Só no fim do fluxo se sabe se é um pack — o
    /// índice está no diretório central, no fim do arquivo —, e é lá que
    /// [`crate::mods::pack::indice_de`] confere.
    Pack,
}

impl Formato {
    /// **O teto do tipo**, que vale desde que o tipo foi lido: 100 MiB de
    /// imagem (`TETO_DE_ARQUIVO`, 0.19) e 64 MiB de pack (`TETO_DO_PACK`).
    fn teto(self) -> u64 {
        match self {
            Self::Imagem(_) => seele_proto::midia_de_mod::TETO_DE_ARQUIVO as u64,
            Self::Pack => seele_proto::pack::TETO_DO_PACK,
        }
    }
}

/// O começo de todo zip que um `.mrpack` é: o cabeçalho local da primeira
/// entrada. É o mesmo teste do começo de [`crate::mods::pack::indice_de`].
const COMECO_DE_ZIP: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

/// O tipo lido dos bytes, e conferido contra o que o MOD declarou aceitar.
fn conferir_tipo(comeco: &[u8], espera: &Espera) -> Result<Formato, Recusa> {
    use seele_proto::volume::VolumeRefusal;
    // **O tipo vem dos bytes**, com a mesma tabela de quatro formatos que a
    // prévia de anexo usa — reúso, e não uma segunda cópia da regra. Decisão 2
    // do ADR 0048: o MOD não tem como conferir o que ele não vê.
    let Some(formato) = seele_proto::imagem::sniff(comeco) else {
        // O `.mrpack`, também pelos bytes, e só onde a espera o aceita: fora
        // disso um zip é «formato-desconhecido», como antes da parte 6.
        let aceita_pack = espera
            .tipos
            .iter()
            .any(|t| t == seele_proto::pack::SUFIXO_DO_PACK);
        if aceita_pack && comeco.starts_with(&COMECO_DE_ZIP) {
            return Ok(Formato::Pack);
        }
        return Err(Recusa {
            motivo: VolumeRefusal::TipoRecusado,
            detectado: Detectado::FormatoNenhum,
        });
    };
    // O MOD declara os tipos como `png`, `jpeg`, `webp`, `gif` — o sufixo do
    // tipo de mídia, que é como o `data:` da janela já os escrevia.
    if !espera.tipos.iter().any(|t| t == formato.sufixo()) {
        return Err(Recusa {
            motivo: VolumeRefusal::TipoRecusado,
            detectado: Detectado::Tipo(formato.sufixo()),
        });
    }
    Ok(Formato::Imagem(formato))
}

/// O corte por teto, com o tamanho e o teto que o MOD lê no nome.
fn acima_do_teto(bytes: u64, teto: u64) -> Recusa {
    Recusa {
        motivo: seele_proto::volume::VolumeRefusal::Incompleto,
        detectado: Detectado::AcimaDoTeto { bytes, teto },
    }
}

/// Os bytes, em blocos, com o hash somado no caminho.
///
/// Devolve um [`Chegou`] com os campos que só os bytes respondem; quem chama
/// preenche o resto.
///
/// Duas coisas valem **durante** o fluxo, e não depois dele:
///
/// - **O teto.** Antes do tipo, o da espera ([`teto_da_espera`], o maior dos
///   tipos que ela aceita); lido o tipo, o dele ([`Formato::teto`]). Passou, a
///   leitura para ali: um fluxo sem fim não enche o disco de quem hospeda. Um
///   total declarado maior que o teto do tipo morre no sniff, antes de o
///   primeiro byte ir ao disco.
/// - **O tipo.** Os primeiros `SNIFF_LEN` bytes ficam retidos, sem tocar o
///   disco, até serem conferidos; um fluxo que não é imagem nem zip (ou não é
///   de um tipo autorizado) morre com no máximo um bloco lido e nada gravado.
///   O `.mrpack` é conferido de novo no fim, pelo índice.
async fn escorrer(
    fluxo: &mut impl Fonte,
    obras: &std::path::Path,
    espera: &Espera,
    tamanho: Option<u64>,
) -> Result<Chegou, Recusa> {
    use seele_proto::attachment::ContentDigest;
    use seele_proto::volume::VolumeRefusal;
    use tokio::io::AsyncWriteExt;

    let mut teto = teto_da_espera(espera);
    let mut arquivo = tokio::fs::File::create(obras)
        .await
        .map_err(|_| VolumeRefusal::NaoGravei)?;
    // O somador do anexo, e não um segundo: o hash de um arquivo neste produto
    // é o mesmo hash, somado pelo mesmo código.
    let mut soma = ContentDigest::new();
    let mut bytes = 0_u64;
    // Os primeiros bytes ficam retidos: é neles que o tipo é lido, e só depois
    // de lidos eles (e o resto) vão para o disco.
    let mut comeco: Vec<u8> = Vec::with_capacity(seele_proto::imagem::SNIFF_LEN);
    let mut formato = None;
    let mut bloco = vec![0_u8; BLOCO];

    loop {
        let lido = fluxo
            .ler(&mut bloco)
            .await
            .map_err(|()| VolumeRefusal::Incompleto)?;
        let Some(quantos) = lido else { break };
        let pedaco = bloco.get(..quantos).unwrap_or_default();
        soma.feed(pedaco);
        bytes = bytes.saturating_add(quantos as u64);
        if tamanho.is_some_and(|total| bytes > total) {
            return Err(VolumeRefusal::Incompleto.into());
        }
        if bytes > teto {
            return Err(acima_do_teto(bytes, teto));
        }
        let mut a_gravar = pedaco;
        if formato.is_none() {
            let falta = seele_proto::imagem::SNIFF_LEN - comeco.len();
            let (cabe, resto) = pedaco.split_at(falta.min(pedaco.len()));
            comeco.extend_from_slice(cabe);
            if comeco.len() < seele_proto::imagem::SNIFF_LEN {
                continue;
            }
            let lido = conferir_tipo(&comeco, espera)?;
            // Daqui em diante, o teto é o do tipo, e o total declarado se
            // confere contra ele antes de o primeiro byte ir ao disco.
            teto = teto.min(lido.teto());
            if let Some(total) = tamanho.filter(|total| *total > teto) {
                return Err(acima_do_teto(total, teto));
            }
            if bytes > teto {
                return Err(acima_do_teto(bytes, teto));
            }
            formato = Some(lido);
            arquivo
                .write_all(&comeco)
                .await
                .map_err(|_| VolumeRefusal::NaoGravei)?;
            a_gravar = resto;
        }
        arquivo
            .write_all(a_gravar)
            .await
            .map_err(|_| VolumeRefusal::NaoGravei)?;
    }
    arquivo
        .flush()
        .await
        .map_err(|_| VolumeRefusal::NaoGravei)?;
    drop(arquivo);

    if tamanho.is_some_and(|total| bytes != total) {
        return Err(VolumeRefusal::Incompleto.into());
    }
    // Um fluxo mais curto que o sniff nunca chegou a conferir: confere agora, e
    // é isso que o recusa como «formato-desconhecido».
    let formato = match formato {
        Some(formato) => formato,
        None => conferir_tipo(&comeco, espera)?,
    };
    let tipo = match formato {
        Formato::Imagem(imagem) => imagem.media_type(),
        Formato::Pack => {
            // **O `.mrpack`** (API 8 parte 6): um zip com o índice na raiz, e
            // um índice que se abre — o arquivo de obras é lido de volta só no
            // diretório central e na entrada do índice. Um zip qualquer, ou
            // um com o começo certo e o resto não, para aqui.
            let e_pack = std::fs::File::open(obras)
                .ok()
                .is_some_and(|mut f| crate::mods::pack::indice_de(&mut f, bytes).is_ok());
            if !e_pack {
                return Err(Recusa {
                    motivo: VolumeRefusal::TipoRecusado,
                    detectado: Detectado::FormatoNenhum,
                });
            }
            seele_proto::pack::TIPO_DO_PACK
        }
    };

    Ok(Chegou {
        mod_id: String::new(),
        pessoa: espera.pessoa,
        token: String::new(),
        caminho: String::new(),
        bytes,
        hash: hex(&soma.finish()),
        tipo,
    })
}

/// **O teto de uma espera antes de o tipo ser lido**: o do maior tipo que ela
/// aceita. Imagem é 100 MiB (0.19) e `.mrpack` 64 MiB (API 8 parte 6), então
/// uma espera de imagens vale 100 MiB, uma só de `mrpack` vale 64, e uma de
/// ambos vale 100 até o sniff dizer qual dos dois chega.
fn teto_da_espera(espera: &Espera) -> u64 {
    let pack = espera
        .tipos
        .iter()
        .any(|t| t == seele_proto::pack::SUFIXO_DO_PACK);
    let imagem = espera
        .tipos
        .iter()
        .any(|t| t != seele_proto::pack::SUFIXO_DO_PACK);
    match (imagem, pack) {
        (false, true) => seele_proto::pack::TETO_DO_PACK,
        (true, true) => TETO_FIXO,
        _ => seele_proto::midia_de_mod::TETO_DE_ARQUIVO as u64,
    }
}

/// O teto da leitura de `alvo`: 64 MiB se os bytes são um `.mrpack`, 100 MiB
/// (o `TETO_DE_ARQUIVO` da 0.19) no resto. Lido do arquivo, e não do nome.
fn teto_da_leitura(alvo: &std::path::Path, tamanho: u64) -> u64 {
    let e_pack = tamanho <= seele_proto::pack::TETO_DO_PACK
        && std::fs::File::open(alvo)
            .ok()
            .is_some_and(|mut f| crate::mods::pack::indice_de(&mut f, tamanho).is_ok());
    if e_pack {
        seele_proto::pack::TETO_DO_PACK
    } else {
        seele_proto::midia_de_mod::TETO_DE_ARQUIVO as u64
    }
}

/// Hexadecimal minúsculo, como todo hash que uma pessoa compara a olho.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut saida, byte| {
        let _ = write!(saida, "{byte:02x}");
        saida
    })
}

/// Cada imagem usa seu próprio fluxo de ida e resposta, inclusive duas leituras simultâneas.
pub async fn atender_imagem(
    server: &std::sync::Arc<crate::server::Server>,
    pessoa: PersonId,
    envio: &mut crate::transporte::Envio,
    entrada: &mut crate::transporte::Recebe,
) -> anyhow::Result<()> {
    use seele_proto::volume::{PedidoDeImagem, RespostaDeImagem};
    // **Um pedido que não passa na validação também tem resposta** (0.19): um
    // envio declarado acima do teto (10 MiB num perfil) era barrado aqui pelo
    // `?` e o fluxo acabava mudo, e quem enviou não sabia por quê.
    let pedido: PedidoDeImagem = match crate::frame::read(entrada).await {
        Ok(pedido) => pedido,
        Err(erro) => {
            tracing::warn!(%pessoa, %erro, "pedido de imagem recusado na leitura");
            let _ = crate::frame::write(
                envio,
                &RespostaDeImagem {
                    erro: Some("pedido-invalido-ou-grande-demais".into()),
                    bytes: 0,
                },
            )
            .await;
            let _ = envio.finish();
            return Err(erro);
        }
    };
    let inicio = Instant::now();
    let resultado: anyhow::Result<Option<tokio::fs::File>> = async {
        match pedido {
            PedidoDeImagem::Enviar { cabecalho, bytes } => {
                let chegou = receber_imagem(&server.esperas, server.mods_dir.as_ref(), pessoa, entrada, cabecalho, Some(bytes)).await
                    .map_err(|(_, recusa)| anyhow::anyhow!("{}", recusa.nome_para_o_mod()))?;
                tracing::info!(mod_id = %chegou.mod_id, bytes = chegou.bytes, em_ms = inicio.elapsed().as_millis(), "imagem de MOD gravada por fluxo");
                Ok(None)
            }
            PedidoDeImagem::Ler { mod_id, channel, payload } => {
                let resposta = crate::mods::pedidos::executar(server, pessoa, channel, &mod_id, &payload).await;
                let valor: serde_json::Value = serde_json::from_str(&resposta)?;
                anyhow::ensure!(valor.get("ok").and_then(serde_json::Value::as_bool) == Some(true), "leitura recusada pelo MOD");
                let caminho = valor.get("volume").and_then(serde_json::Value::as_str).ok_or_else(|| anyhow::anyhow!("resposta sem volume"))?;
                let raiz = server.mods_dir.as_ref().ok_or_else(|| anyhow::anyhow!("sem diretório de MODs"))?.dados_de(&mod_id).join(PASTA);
                let alvo = crate::mods::arquivos::dentro(&raiz, caminho).ok_or_else(|| anyhow::anyhow!("caminho de volume inválido"))?;
                let arquivo = tokio::fs::File::open(&alvo).await?;
                let tamanho = arquivo.metadata().await?.len();
                // O teto da leitura é o do tipo dos bytes (API 8 parte 6): o
                // `.mrpack` vai até 64 MiB, para a máquina e para quem joga.
                let teto = {
                    let alvo = alvo.clone();
                    tokio::task::spawn_blocking(move || teto_da_leitura(&alvo, tamanho)).await?
                };
                anyhow::ensure!(tamanho <= teto, "arquivo-grande-demais:{tamanho}:{teto}");
                Ok(Some(arquivo))
            }
            // **As imagens do perfil** (0.19) não passam por MOD: o teto é o
            // `TETO_DA_IMAGEM` (10 MiB), o tipo vem dos bytes, e quem envia é
            // sempre `pessoa`. Tudo mora em `perfis/imagens.rs`.
            PedidoDeImagem::EnviarAoPerfil { lado, bytes } => {
                crate::perfis::imagens::atender_envio(server, pessoa, lado, bytes, entrada).await?;
                Ok(None)
            }
            PedidoDeImagem::LerDoPerfil { pessoa: de_quem, lado, hash } => {
                Ok(Some(crate::perfis::imagens::atender_leitura(server, de_quem, lado, hash).await?))
            }
        }
    }.await;
    match resultado {
        Ok(arquivo) => {
            let bytes = match &arquivo {
                Some(f) => f.metadata().await?.len(),
                None => 0,
            };
            crate::frame::write(envio, &RespostaDeImagem { erro: None, bytes }).await?;
            if let Some(mut arquivo) = arquivo {
                tokio::io::copy(&mut arquivo, envio).await?;
            }
        }
        Err(erro) => {
            tracing::warn!(%pessoa, %erro, "transferência de imagem de MOD recusada");
            crate::frame::write(
                envio,
                &RespostaDeImagem {
                    erro: Some(erro.to_string().chars().take(512).collect()),
                    bytes: 0,
                },
            )
            .await?;
        }
    }
    envio.finish()?;
    Ok(())
}
