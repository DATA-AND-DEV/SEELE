//! As máquinas de jogo que a pessoa aceitou rodar (API 8 parte 3).
//!
//! Vizinho de [`crate::aceites`], e separado dele pela mesma razão que aquele
//! é separado dos pins: são decisões diferentes. Aceitar os MODs de um
//! servidor é deixar uma janela abrir; aceitar uma máquina é deixar este
//! computador baixar e executar o que o dono do servidor descreveu. Apagar um
//! não pode apagar o outro.
//!
//! # O que se guarda é o que foi mostrado
//!
//! Cada linha leva a oferta inteira, e não só o resumo. O resumo
//! ([`seele_proto::maquina::resumo_da_oferta`]) é o que decide se um pedido
//! novo é o mesmo que a pessoa já aceitou; a oferta é o que a parte 4 lê para
//! saber **o que** foi aceito: de onde baixar (`origens`, `artefatos`), o que
//! executar (`rodar`) e quanto disco ocupar (`disco_maximo`). Guardar só o
//! resumo obrigaria a parte 4 a confiar no que o servidor manda depois — e o
//! aceite existe justamente para ela não precisar.
//!
//! # Formato
//!
//! `<config>/maquinas`, uma linha JSON por aceite. JSON e não tabulação
//! (como os aceites de MODs) porque a oferta tem listas e campos opcionais; e
//! uma linha por aceite para que uma linha estragada à mão custe só ela.
//!
//! A escrita é atômica: o arquivo novo nasce ao lado e troca de nome por cima
//! do velho. Uma queda no meio da escrita deixa o arquivo de antes, e não meio
//! arquivo — meio arquivo seria aceites perdidos sem ninguém ter revogado.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use seele_proto::maquina::OfertaDeMaquina;
use serde::{Deserialize, Serialize};

/// O nome do arquivo, dentro do diretório de configuração.
const ARQUIVO: &str = "maquinas";

/// Um aceite de máquina guardado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AceiteDeMaquina {
    /// Sob que chave o servidor SEELE é arquivado: a mesma do pin e dos
    /// aceites de MODs.
    pub alvo: String,
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O hash do pacote do MOD que estava ligado quando a pessoa aceitou.
    pub hash: String,
    /// O servidor de jogo.
    pub servidor: String,
    /// O resumo do que foi mostrado, em hex: o que o `Aviso` trouxe, nunca
    /// recalculado aqui.
    pub resumo: String,
    /// A oferta inteira, como a pessoa a leu.
    pub oferta: OfertaDeMaquina,
    /// O apelido de quem pediu, como a tela o mostrou.
    pub pedido_por: String,
    /// Quando, em segundos desde a época.
    pub quando: u64,
}

impl AceiteDeMaquina {
    fn e_de(&self, alvo: &str, mod_id: &str, servidor: &str) -> bool {
        self.alvo == alvo && self.mod_id == mod_id && self.servidor == servidor
    }
}

/// O arquivo de aceites de máquina de uma instalação.
#[derive(Debug)]
pub struct AceitesDeMaquina {
    arquivo: PathBuf,
    /// Quantas linhas a última leitura pulou. Um átomo para `listar` seguir
    /// com `&self`, como os vizinhos.
    ilegiveis: AtomicUsize,
}

impl AceitesDeMaquina {
    /// Os aceites de máquina guardados no diretório de configuração.
    #[must_use]
    pub fn em(diretorio: &Path) -> Self {
        Self {
            arquivo: diretorio.join(ARQUIVO),
            ilegiveis: AtomicUsize::new(0),
        }
    }

    /// Todo aceite legível.
    ///
    /// Sem arquivo, lista vazia: é o estado de quem nunca aceitou máquina
    /// nenhuma. Uma linha que não se lê é pulada e contada em
    /// [`Self::ilegiveis`] — pulada porque perder as outras por causa dela
    /// seria pior, e contada porque um aceite que some calado volta dias
    /// depois como «por que ele me perguntou de novo?», sem dado nenhum.
    ///
    /// # Errors
    ///
    /// Qualquer falha de leitura que não seja «o arquivo não existe».
    pub fn listar(&self) -> io::Result<Vec<AceiteDeMaquina>> {
        let texto = match std::fs::read_to_string(&self.arquivo) {
            Ok(texto) => texto,
            Err(erro) if erro.kind() == io::ErrorKind::NotFound => {
                self.ilegiveis.store(0, Ordering::Relaxed);
                return Ok(Vec::new());
            }
            Err(erro) => return Err(erro),
        };
        let mut aceites = Vec::new();
        let mut ilegiveis = 0;
        for linha in texto.lines().filter(|linha| !linha.trim().is_empty()) {
            match serde_json::from_str::<AceiteDeMaquina>(linha) {
                Ok(aceite) => aceites.push(aceite),
                Err(_) => ilegiveis += 1,
            }
        }
        self.ilegiveis.store(ilegiveis, Ordering::Relaxed);
        Ok(aceites)
    }

    /// Quantas linhas a última leitura ([`Self::listar`], e as que a usam)
    /// pulou por não se lerem.
    #[must_use]
    pub fn ilegiveis(&self) -> usize {
        self.ilegiveis.load(Ordering::Relaxed)
    }

    /// O aceite desta máquina, se houver.
    ///
    /// # Errors
    ///
    /// As de [`Self::listar`].
    pub fn de(
        &self,
        alvo: &str,
        mod_id: &str,
        servidor: &str,
    ) -> io::Result<Option<AceiteDeMaquina>> {
        Ok(self
            .listar()?
            .into_iter()
            .find(|aceite| aceite.e_de(alvo, mod_id, servidor)))
    }

    /// Guarda um aceite, no lugar do de mesma chave `(alvo, mod_id,
    /// servidor)`.
    ///
    /// Substitui em vez de acumular pela razão dos aceites de MODs: um aceite
    /// velho guardado ao lado do novo seria permissão para uma oferta que a
    /// pessoa não leu mais.
    ///
    /// **As linhas ilegíveis se perdem aqui.** Reescrevê-las como vieram
    /// manteria no arquivo algo que nenhuma leitura usa; a contagem de
    /// [`Self::ilegiveis`] é o rastro delas, e o app a põe no log.
    ///
    /// # Errors
    ///
    /// As de [`Self::listar`], e a escrita.
    pub fn guardar(&self, aceite: &AceiteDeMaquina) -> io::Result<()> {
        let _trava = self.travar()?;
        let mut aceites: Vec<AceiteDeMaquina> = self
            .listar()?
            .into_iter()
            .filter(|velho| !velho.e_de(&aceite.alvo, &aceite.mod_id, &aceite.servidor))
            .collect();
        self.dizer_as_ilegiveis_que_saem("guardar");
        aceites.push(aceite.clone());
        self.escrever(&aceites)
    }

    /// **As linhas ilegíveis que a regravação vai tirar, no log** (revisão
    /// final, M6): `guardar` e `revogar` reescrevem o arquivo só com o que se
    /// leu, e a contagem de [`Self::ilegiveis`] ficava só no contador — que
    /// ninguém lia nesses dois caminhos. Quem procura um aceite que sumiu acha
    /// aqui quantas linhas saíram, e de onde.
    fn dizer_as_ilegiveis_que_saem(&self, onde: &str) {
        let ilegiveis = self.ilegiveis();
        if ilegiveis > 0 {
            tracing::warn!(
                arquivo = %self.arquivo.display(),
                ilegiveis,
                onde,
                "aceites de máquina: linhas que não se leem saem do arquivo ao regravar"
            );
        }
    }

    /// Apaga o aceite desta máquina. `true` se havia um.
    ///
    /// # Errors
    ///
    /// As de [`Self::listar`], e a escrita.
    pub fn revogar(&self, alvo: &str, mod_id: &str, servidor: &str) -> io::Result<bool> {
        let _trava = self.travar()?;
        let antes = self.listar()?;
        let total = antes.len();
        let depois: Vec<AceiteDeMaquina> = antes
            .into_iter()
            .filter(|aceite| !aceite.e_de(alvo, mod_id, servidor))
            .collect();
        if depois.len() == total {
            return Ok(false);
        }
        self.dizer_as_ilegiveis_que_saem("revogar");
        self.escrever(&depois)?;
        Ok(true)
    }

    /// A trava de `maquinas.trava`, segurada da leitura até a troca.
    ///
    /// **Sem ela, uma revogação podia voltar.** `guardar` e `revogar` leem o
    /// arquivo inteiro e o regravam; um `guardar` que leu antes de uma
    /// revogação terminar regravava a lista velha — com o aceite revogado
    /// dentro, e é ele que a parte 4 usa para baixar e executar. A mesma trava
    /// dos `conhecidos` e da agenda (um `flock` num arquivo ao lado, que vale
    /// entre processos e entre threads do mesmo processo).
    fn travar(&self) -> io::Result<Option<std::fs::File>> {
        if let Some(pasta) = self.arquivo.parent() {
            std::fs::create_dir_all(pasta)?;
        }
        crate::conhecidos::travar(&self.arquivo).map_err(io::Error::other)
    }

    /// Regrava o arquivo inteiro. Quem chama segura [`Self::travar`].
    ///
    /// Pelo `trocar_privado` dos vizinhos: o arquivo novo nasce `0600` ao lado
    /// e troca de nome por cima do velho. `0600` porque o que está aqui diz o
    /// que este computador aceita executar, e não é da conta de outra conta da
    /// máquina; a troca, para uma queda no meio deixar o arquivo de antes.
    fn escrever(&self, aceites: &[AceiteDeMaquina]) -> io::Result<()> {
        let mut texto = String::new();
        for aceite in aceites {
            let linha = serde_json::to_string(aceite).map_err(io::Error::other)?;
            texto.push_str(&linha);
            texto.push('\n');
        }
        crate::conhecidos::trocar_privado(&self.arquivo, texto.as_bytes()).map_err(io::Error::other)
    }
}

/// Onde mora a pasta de um servidor de jogo (Desvio 10):
/// `<config>/jogos/<alvo>/<mod-id com / trocado por _>/<servidor>/`.
///
/// **Não cria nada**: quem cria é a parte 4, na hora de baixar. Aqui só se
/// decide o nome, para o aceite poder dizer «onde» antes de existir.
///
/// Cada pedaço passa por [`pedaco_seguro`]: `alvo` e `mod_id` vêm do servidor
/// e da casca, e um `autor` igual a `..` (que `id_de_mod_valido` aceita) não
/// pode subir para fora de `jogos/` — muito menos para o diretório onde mora
/// `identity.key`.
#[must_use]
pub fn pasta_do_servidor_de_jogo(
    dir_de_config: &Path,
    alvo: &str,
    mod_id: &str,
    servidor: &str,
) -> PathBuf {
    dir_de_config
        .join("jogos")
        .join(pedaco_seguro(alvo))
        .join(pedaco_seguro(mod_id))
        .join(pedaco_seguro(servidor))
}

/// Um nome de pasta que não sai de onde foi posto e não colide com outro.
///
/// Fica `[a-z0-9.-]`; `/` vira `_` (a forma que o Desvio 10 dá ao MOD); todo
/// o resto, inclusive `_` e maiúsculas, vira `%XX`. Maiúsculas também porque o
/// sistema de arquivos do Mac não as distingue, e `A` e `a` seriam a mesma
/// pasta para dois nomes diferentes. Um `.` no começo vira `%2E`: assim `.` e
/// `..` nunca saem como estão.
fn pedaco_seguro(texto: &str) -> String {
    use std::fmt::Write as _;
    let mut saida = String::with_capacity(texto.len());
    for (i, byte) in texto.bytes().enumerate() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'-' => saida.push(char::from(byte)),
            b'.' if i > 0 => saida.push('.'),
            b'/' => saida.push('_'),
            _ => {
                let _ = write!(saida, "%{byte:02X}");
            }
        }
    }
    if saida.is_empty() {
        saida.push('%');
    }
    // Os nomes de aparelho do Windows (`con`, `nul`, `com1`...) não são pastas
    // lá, nem com extensão: `nul.x` abre o aparelho. O primeiro byte escapado
    // basta para o nome deixar de sê-lo, e continua sem colidir.
    let tronco = saida.split('.').next().unwrap_or_default();
    if nome_de_aparelho_do_windows(tronco) {
        let primeiro = saida.remove(0);
        saida.insert_str(0, &format!("%{:02X}", u32::from(primeiro)));
    }
    saida
}

fn nome_de_aparelho_do_windows(tronco: &str) -> bool {
    match tronco {
        "con" | "prn" | "aux" | "nul" => true,
        _ => {
            let (base, numero) = tronco.split_at(tronco.len().min(3));
            matches!(base, "com" | "lpt")
                && numero.len() == 1
                && numero.bytes().all(|b| (b'1'..=b'9').contains(&b))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_proto::maquina::{ComandoOferecido, TunelOferecido};
    use seele_proto::tunel::ProtocoloDeTunel;

    fn pasta(nome: &str) -> PathBuf {
        static QUAL: AtomicUsize = AtomicUsize::new(0);
        let n = QUAL.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("seele-maquinas-{nome}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    fn oferta(porta: u16) -> OfertaDeMaquina {
        OfertaDeMaquina {
            titulo: "Minecraft".into(),
            receita: "minecraft-java".into(),
            versao_da_receita: "1.0.0".into(),
            versao_do_jogo: "1.21.4".into(),
            tuneis: vec![TunelOferecido {
                nome: "mc".into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta,
            }],
            artefatos: vec![],
            pack: None,
            origens: vec!["piston-data.mojang.com".into()],
            dominios_do_jogo: vec![],
            rodar: Some(ComandoOferecido {
                comando: "java".into(),
                argumentos: vec!["-jar".into(), "server.jar".into()],
            }),
            disco_maximo: 1024,
            termos: vec![],
            instalar: None,
            por_sistema: vec![],
        }
    }

    fn aceite(alvo: &str, servidor: &str, porta: u16) -> AceiteDeMaquina {
        AceiteDeMaquina {
            alvo: alvo.into(),
            mod_id: "seele/jogo".into(),
            hash: "ab".repeat(32),
            servidor: servidor.into(),
            resumo: "cd".repeat(32),
            oferta: oferta(porta),
            pedido_por: "marcela".into(),
            quando: 1_789_092_000,
        }
    }

    #[test]
    fn guardar_e_ler_de_volta() {
        let aceites = AceitesDeMaquina::em(&pasta("volta"));
        assert_eq!(
            aceites.de("casa:8383", "seele/jogo", "mundo").unwrap(),
            None
        );
        let guardado = aceite("casa:8383", "mundo", 25565);
        aceites.guardar(&guardado).unwrap();
        assert_eq!(
            aceites.de("casa:8383", "seele/jogo", "mundo").unwrap(),
            Some(guardado),
            "o aceite não voltou inteiro: a parte 4 lê a oferta daqui"
        );
        assert_eq!(aceites.ilegiveis(), 0);
    }

    /// Um aceite novo da mesma máquina substitui o velho: guardar os dois
    /// seria permissão para uma oferta que a pessoa não leu mais.
    #[test]
    fn guardar_de_novo_substitui() {
        let aceites = AceitesDeMaquina::em(&pasta("substitui"));
        aceites
            .guardar(&aceite("casa:8383", "mundo", 25565))
            .unwrap();
        aceites
            .guardar(&aceite("casa:8383", "mundo", 25566))
            .unwrap();
        let todos = aceites.listar().unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].oferta.tuneis[0].porta, 25566);
    }

    #[test]
    fn revogar_apaga_so_aquele() {
        let aceites = AceitesDeMaquina::em(&pasta("revogar"));
        aceites.guardar(&aceite("casa:8383", "mundo", 1)).unwrap();
        aceites.guardar(&aceite("casa:8383", "outro", 2)).unwrap();
        aceites
            .guardar(&aceite("trabalho:8383", "mundo", 3))
            .unwrap();

        assert!(aceites.revogar("casa:8383", "seele/jogo", "mundo").unwrap());
        assert!(
            !aceites.revogar("casa:8383", "seele/jogo", "mundo").unwrap(),
            "revogar de novo disse que havia o que apagar"
        );
        assert_eq!(
            aceites.de("casa:8383", "seele/jogo", "mundo").unwrap(),
            None
        );
        assert!(aceites
            .de("casa:8383", "seele/jogo", "outro")
            .unwrap()
            .is_some());
        assert!(aceites
            .de("trabalho:8383", "seele/jogo", "mundo")
            .unwrap()
            .is_some());
    }

    /// O arquivo é texto; uma linha estragada à mão custa só ela, e a conta
    /// diz quantas foram.
    #[test]
    fn uma_linha_ilegivel_e_pulada_e_contada() {
        let dir = pasta("ilegivel");
        let aceites = AceitesDeMaquina::em(&dir);
        let boa = serde_json::to_string(&aceite("casa:8383", "mundo", 1)).unwrap();
        std::fs::write(
            dir.join(ARQUIVO),
            format!("{{isto não é json\n{boa}\n{{\"alvo\":\"x\"}}\n"),
        )
        .unwrap();

        let lidos = aceites.listar().unwrap();
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].alvo, "casa:8383");
        assert_eq!(aceites.ilegiveis(), 2);
    }

    /// **Regravar tira as ilegíveis, e o log diz quantas** (revisão final,
    /// M6): `guardar` e `revogar` deixam um `WARN` com a contagem.
    #[test]
    fn as_ilegiveis_que_a_regravacao_tira_vao_ao_log() {
        let dir = pasta("ilegivel-no-log");
        let aceites = AceitesDeMaquina::em(&dir);
        let boa = serde_json::to_string(&aceite("casa:8383", "mundo", 1)).unwrap();
        std::fs::write(dir.join(ARQUIVO), format!("{{quebrada\n{boa}\n")).unwrap();
        let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::WARN);
        let _guarda = tracing::subscriber::set_default(rastro.clone());
        aceites.guardar(&aceite("casa:8383", "outro", 2)).unwrap();
        let linhas = rastro.linhas();
        assert!(
            linhas.iter().any(|l| l.contains("saem do arquivo")
                && l.contains("ilegiveis=1")
                && l.contains("guardar")),
            "{linhas:?}"
        );
        std::fs::write(dir.join(ARQUIVO), format!("{{quebrada\n{boa}\n")).unwrap();
        assert!(aceites.revogar("casa:8383", "seele/jogo", "mundo").unwrap());
        assert!(
            rastro
                .linhas()
                .iter()
                .any(|l| l.contains("ilegiveis=1") && l.contains("revogar")),
            "{:?}",
            rastro.linhas()
        );
    }

    /// A pasta onde o jogo vai morar nunca é a de configuração, nem a contém:
    /// lá mora `identity.key`, e um jogo que a lesse levaria a identidade da
    /// pessoa. Nem com nomes que tentam subir.
    #[test]
    fn a_pasta_do_jogo_nunca_e_a_de_configuracao() {
        let dir = PathBuf::from("/config/seele");
        let chave = dir.join("identity.key");
        for (alvo, mod_id, servidor) in [
            ("casa:8383", "seele/jogo", "mundo"),
            ("..", "../..", ".."),
            ("", "./.", "."),
            ("[::1]:8383", "Autor/Nome", "mundo"),
        ] {
            let pasta = pasta_do_servidor_de_jogo(&dir, alvo, mod_id, servidor);
            assert_ne!(pasta, dir);
            assert!(
                pasta.starts_with(dir.join("jogos")),
                "{pasta:?} saiu de jogos/"
            );
            assert!(!chave.starts_with(&pasta), "{pasta:?} contém identity.key");
            for pedaco in pasta.strip_prefix(&dir).unwrap().components() {
                assert!(
                    matches!(pedaco, std::path::Component::Normal(_)),
                    "{pasta:?} tem um pedaço que não é nome: {pedaco:?}"
                );
            }
        }
        assert_eq!(
            pasta_do_servidor_de_jogo(&dir, "casa:8383", "seele/jogo", "mundo"),
            dir.join("jogos/casa%3A8383/seele_jogo/mundo")
        );
    }

    /// Os nomes de aparelho do Windows saem escapados, com ou sem extensão, e
    /// os vizinhos deles não.
    #[test]
    fn um_nome_de_aparelho_do_windows_nao_vira_pasta() {
        for nome in [
            "con", "prn", "aux", "nul", "com1", "lpt9", "nul.txt", "con.d",
        ] {
            let pedaco = pedaco_seguro(nome);
            assert!(pedaco.starts_with('%'), "{nome} saiu {pedaco}");
        }
        for nome in ["console", "com0", "com10", "lpt", "nulo"] {
            assert_eq!(pedaco_seguro(nome), nome);
        }
        assert_ne!(pedaco_seguro("con"), pedaco_seguro("%63on"));
    }

    /// **Uma revogação não volta.** `guardar` e `revogar` ao mesmo tempo, em
    /// threads: sem a trava entre a leitura e a troca, um `guardar` que leu a
    /// lista antes da revogação a regravava com o aceite revogado dentro.
    #[test]
    fn guardar_e_revogar_ao_mesmo_tempo_nao_ressuscitam_o_revogado() {
        let dir = pasta("corrida");
        let mut ressuscitados = 0;
        for volta in 0..200 {
            let aceites = AceitesDeMaquina::em(&dir);
            aceites.guardar(&aceite("casa:8383", "mundo", 1)).unwrap();
            std::thread::scope(|escopo| {
                escopo.spawn(|| {
                    AceitesDeMaquina::em(&dir)
                        .guardar(&aceite("casa:8383", &format!("outro-{volta}"), 2))
                        .unwrap();
                });
                escopo.spawn(|| {
                    AceitesDeMaquina::em(&dir)
                        .revogar("casa:8383", "seele/jogo", "mundo")
                        .unwrap();
                });
            });
            let depois = AceitesDeMaquina::em(&dir);
            if depois
                .de("casa:8383", "seele/jogo", "mundo")
                .unwrap()
                .is_some()
            {
                ressuscitados += 1;
            }
            assert!(
                depois
                    .de("casa:8383", "seele/jogo", &format!("outro-{volta}"))
                    .unwrap()
                    .is_some(),
                "o aceite guardado ao mesmo tempo se perdeu (volta {volta})"
            );
            assert_eq!(depois.ilegiveis(), 0, "meio arquivo lido (volta {volta})");
        }
        assert_eq!(
            ressuscitados, 0,
            "o aceite revogado voltou em {ressuscitados} de 200"
        );
    }

    /// O arquivo diz o que este computador aceita executar: só a conta o lê.
    #[cfg(unix)]
    #[test]
    fn o_arquivo_nasce_so_da_conta() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = pasta("modo");
        AceitesDeMaquina::em(&dir)
            .guardar(&aceite("casa:8383", "mundo", 1))
            .unwrap();
        let modo = std::fs::metadata(dir.join(ARQUIVO))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(modo & 0o777, 0o600);
    }

    /// Dois nomes diferentes nunca dão a mesma pasta.
    #[test]
    fn pedacos_diferentes_dao_pastas_diferentes() {
        assert_ne!(pedaco_seguro("a/b"), pedaco_seguro("a_b"));
        assert_ne!(pedaco_seguro("A"), pedaco_seguro("a"));
        assert_ne!(pedaco_seguro(".x"), pedaco_seguro("%2Ex"));
    }
}
