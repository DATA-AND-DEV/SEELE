//! A máquina de jogo (protocolo 12, API 8 parte 3): o que o servidor mostra a
//! quem opera uma máquina, o que essa pessoa responde, e por que uma máquina
//! para ou é dispensada.
//!
//! Aqui só mora o que atravessa o fio ou nomeia um motivo; quem decide, quem
//! baixa e quem executa estão nos crates que usam estes tipos.
//!
//! **O aceite é sobre o que foi mostrado.** [`resumo_da_oferta`] amarra numa
//! impressão só o MOD, o pacote ligado, o servidor e cada coisa da oferta: se o
//! dono do servidor trocar uma porta, um hash ou um argumento depois de a
//! pessoa ler, o resumo muda e o aceite dela deixa de valer.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::control::{check_bounds, ControlError, Validate};
use crate::tunel::ProtocoloDeTunel;

/// O maior nome de servidor de jogo. `[a-z0-9-]`, como um nome de túnel: ele
/// viaja em chaves e em logs.
pub const MAX_SERVIDOR: usize = 32;
/// O maior título de uma oferta, em bytes.
pub const MAX_TITULO: usize = 64;
/// Quantos túneis uma oferta declara.
pub const MAX_TUNEIS_NA_OFERTA: usize = 8;
/// Quantos artefatos uma oferta manda baixar.
pub const MAX_ARTEFATOS: usize = 16;
/// Quantas origens de rede uma oferta declara.
pub const MAX_ORIGENS: usize = 16;
/// Quantos domínios do jogo uma oferta declara.
pub const MAX_DOMINIOS_DO_JOGO: usize = 16;
/// Quantos argumentos leva o comando de uma oferta.
pub const MAX_ARGUMENTOS: usize = 64;
/// Quantos termos a pessoa precisa aceitar.
pub const MAX_TERMOS: usize = 4;
/// O teto de cada texto livre de uma oferta, em bytes.
pub const MAX_TEXTO: usize = 256;
/// O teto de cada URL de uma oferta, em bytes.
pub const MAX_URL: usize = 512;
/// O maior disco que uma oferta pode pedir: 64 GiB.
pub const MAX_DISCO: u64 = 64 * 1024 * 1024 * 1024;

/// Se `s` é um nome de servidor de jogo: `[a-z0-9-]`, de 1 a [`MAX_SERVIDOR`],
/// sem `-` nas pontas. A mesma régua de [`crate::tunel::nome_valido`].
#[must_use]
pub fn servidor_valido(s: &str) -> bool {
    s.len() <= MAX_SERVIDOR && crate::tunel::nome_valido(s)
}

/// O algoritmo de um hash de artefato.
///
/// Os jogos publicam o hash que têm: SHA-1 (o `server.jar` do Minecraft),
/// SHA-256 e SHA-512. **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlgoritmoDeHash {
    /// 40 caracteres em hex.
    Sha1,
    /// 64 caracteres em hex.
    Sha256,
    /// 128 caracteres em hex.
    Sha512,
}

impl AlgoritmoDeHash {
    /// Quantos caracteres tem o hash em hexadecimal.
    #[must_use]
    pub const fn tamanho_em_hex(self) -> usize {
        match self {
            Self::Sha1 => 40,
            Self::Sha256 => 64,
            Self::Sha512 => 128,
        }
    }
}

/// Um hash de artefato: o algoritmo e o hex minúsculo.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HashDeArtefato {
    /// Com que se calculou.
    pub algoritmo: AlgoritmoDeHash,
    /// Hexadecimal minúsculo, do tamanho que o algoritmo manda.
    pub hex: String,
}

impl HashDeArtefato {
    /// Se o hex tem o tamanho do algoritmo e só `0-9a-f`.
    ///
    /// Caixa fixa pela mesma razão de [`crate::mods::e_hash_de_conteudo`]: o
    /// resumo é calculado sobre este texto, e `AB` e `ab` seriam dois resumos
    /// para os mesmos bytes.
    #[must_use]
    pub fn e_valido(&self) -> bool {
        self.hex.len() == self.algoritmo.tamanho_em_hex()
            && self
                .hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b.is_ascii_lowercase() && b.is_ascii_hexdigit()))
    }
}

/// Um túnel que a máquina vai servir.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TunelOferecido {
    /// O nome do túnel ([`crate::tunel::nome_valido`]).
    pub nome: String,
    /// O que ele carrega.
    pub protocolo: ProtocoloDeTunel,
    /// A porta em que o jogo escuta, em `127.0.0.1`.
    pub porta: u16,
}

/// Um arquivo que a máquina vai baixar.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtefatoOferecido {
    /// O nome com que ele fica no disco.
    pub nome: String,
    /// De onde baixar; sempre `https://`.
    pub url: String,
    /// O tamanho esperado, em bytes.
    pub tamanho: u64,
    /// O hash esperado.
    pub hash: HashDeArtefato,
}

/// O comando que a máquina vai executar.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComandoOferecido {
    /// O programa.
    pub comando: String,
    /// Os argumentos, um a um: nenhum shell no meio.
    pub argumentos: Vec<String>,
}

/// Um termo que a pessoa aceita ao aceitar a máquina (por exemplo, o EULA do
/// jogo).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TermoOferecido {
    /// Como ele se chama.
    pub titulo: String,
    /// Onde ler; sempre `https://`.
    pub url: String,
}

/// Tudo o que uma pessoa vê antes de aceitar rodar uma máquina.
///
/// **Já carrega o que a parte 4 vai usar** (`pack`, `origens`,
/// `dominios_do_jogo`, `rodar`, `disco_maximo`, `termos`): campo novo numa
/// mensagem que existe nunca entra (ADR 0058), e o 12 sai com a oferta inteira.
/// O que a parte 3 ainda não exerce viaja vazio.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OfertaDeMaquina {
    /// Como a pessoa chama esta máquina.
    pub titulo: String,
    /// O nome da receita que a monta.
    pub receita: String,
    /// A versão da receita.
    pub versao_da_receita: String,
    /// A versão do jogo que a receita instala.
    pub versao_do_jogo: String,
    /// Os túneis que ela serve.
    pub tuneis: Vec<TunelOferecido>,
    /// O que ela baixa.
    pub artefatos: Vec<ArtefatoOferecido>,
    /// O hash do pacote de arquivos do jogo, se houver.
    pub pack: Option<HashDeArtefato>,
    /// De onde a máquina pode baixar (hosts, sem esquema).
    pub origens: Vec<String>,
    /// Com quem o jogo pode falar (hosts, sem esquema).
    pub dominios_do_jogo: Vec<String>,
    /// O que ela executa.
    pub rodar: Option<ComandoOferecido>,
    /// O disco que ela pode ocupar, em bytes.
    pub disco_maximo: u64,
    /// O que a pessoa precisa aceitar além da oferta.
    pub termos: Vec<TermoOferecido>,
}

fn fora(field: &'static str) -> ControlError {
    ControlError::FieldOutOfRange { field }
}

/// Um texto livre: não vazio, até `teto` bytes, sem caractere de controle.
fn texto(field: &'static str, s: &str, teto: usize) -> Result<(), ControlError> {
    check_bounds(field, s.len(), teto)?;
    if s.is_empty() || s.chars().any(char::is_control) {
        return Err(fora(field));
    }
    Ok(())
}

fn url(field: &'static str, s: &str) -> Result<(), ControlError> {
    check_bounds(field, s.len(), MAX_URL)?;
    match s.strip_prefix("https://") {
        Some(resto)
            if !resto.is_empty() && !s.chars().any(|c| c.is_control() || c.is_whitespace()) =>
        {
            Ok(())
        }
        _ => Err(fora(field)),
    }
}

/// Um nome de host, sem esquema, sem porta e sem `/`. `*.` no começo vale.
fn host(field: &'static str, s: &str) -> Result<(), ControlError> {
    check_bounds(field, s.len(), MAX_TEXTO)?;
    let sem_curinga = s.strip_prefix("*.").unwrap_or(s);
    if sem_curinga.is_empty()
        || !sem_curinga
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
    {
        return Err(fora(field));
    }
    Ok(())
}

fn hash(field: &'static str, h: &HashDeArtefato) -> Result<(), ControlError> {
    if h.e_valido() {
        Ok(())
    } else {
        Err(fora(field))
    }
}

impl Validate for OfertaDeMaquina {
    fn validate(&self) -> Result<(), ControlError> {
        texto("titulo", &self.titulo, MAX_TITULO)?;
        texto("receita", &self.receita, MAX_TEXTO)?;
        texto("versao_da_receita", &self.versao_da_receita, MAX_TEXTO)?;
        texto("versao_do_jogo", &self.versao_do_jogo, MAX_TEXTO)?;

        check_bounds("tuneis", self.tuneis.len(), MAX_TUNEIS_NA_OFERTA)?;
        for (i, t) in self.tuneis.iter().enumerate() {
            if !crate::tunel::nome_valido(&t.nome) || t.porta == 0 {
                return Err(fora("tuneis"));
            }
            if self.tuneis.iter().take(i).any(|antes| antes.nome == t.nome) {
                return Err(fora("tuneis"));
            }
        }

        check_bounds("artefatos", self.artefatos.len(), MAX_ARTEFATOS)?;
        for a in &self.artefatos {
            texto("artefato_nome", &a.nome, MAX_TEXTO)?;
            url("artefato_url", &a.url)?;
            if a.tamanho == 0 || a.tamanho > MAX_DISCO {
                return Err(fora("artefato_tamanho"));
            }
            hash("artefato_hash", &a.hash)?;
        }
        if let Some(pack) = &self.pack {
            hash("pack", pack)?;
        }

        check_bounds("origens", self.origens.len(), MAX_ORIGENS)?;
        for o in &self.origens {
            host("origens", o)?;
        }
        check_bounds(
            "dominios_do_jogo",
            self.dominios_do_jogo.len(),
            MAX_DOMINIOS_DO_JOGO,
        )?;
        for d in &self.dominios_do_jogo {
            host("dominios_do_jogo", d)?;
        }

        if let Some(rodar) = &self.rodar {
            texto("comando", &rodar.comando, MAX_TEXTO)?;
            check_bounds("argumentos", rodar.argumentos.len(), MAX_ARGUMENTOS)?;
            for argumento in &rodar.argumentos {
                // Um argumento vazio é legítimo (`--flag ""`); o teto e a
                // ausência de controle valem.
                check_bounds("argumento", argumento.len(), MAX_TEXTO)?;
                if argumento.chars().any(char::is_control) {
                    return Err(fora("argumento"));
                }
            }
        }

        if self.disco_maximo > MAX_DISCO {
            return Err(fora("disco_maximo"));
        }
        check_bounds("termos", self.termos.len(), MAX_TERMOS)?;
        for t in &self.termos {
            texto("termo_titulo", &t.titulo, MAX_TEXTO)?;
            url("termo_url", &t.url)?;
        }
        Ok(())
    }
}

/// O resumo do que a pessoa viu: SHA-256 da codificação do fio (postcard, a
/// mesma dos quadros de controle) de `(mod_id, hash_do_pacote, servidor,
/// oferta)`, atrás de um rótulo de domínio.
///
/// Determinístico porque o postcard o é: nenhum mapa, nenhum campo opcional
/// escrito de dois jeitos. O aceite guarda este resumo, e não a oferta, então
/// qualquer coisa que a oferta mostre e que não entrasse aqui poderia mudar
/// depois do aceite sem invalidá-lo.
#[must_use]
pub fn resumo_da_oferta(
    mod_id: &str,
    hash_do_pacote: &str,
    servidor: &str,
    oferta: &OfertaDeMaquina,
) -> [u8; 32] {
    const ROTULO: &[u8] = b"seele/oferta-de-maquina/1\0";
    let corpo = (mod_id, hash_do_pacote, servidor, oferta);
    // `to_extend` num `Vec` só falha por falta de memória, que aborta antes.
    let bytes = postcard::to_extend(&corpo, ROTULO.to_vec()).unwrap_or_default();
    Sha256::digest(&bytes).into()
}

/// O resumo em hexadecimal minúsculo, 64 caracteres: a forma que viaja em
/// [`crate::control::ClientMessage::AceiteDeMaquina`].
#[must_use]
pub fn resumo_em_hex(resumo: &[u8; 32]) -> String {
    crate::mods::hex(resumo)
}

/// Por que a pessoa não rodou a máquina.
///
/// **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotivoDaRecusaDeMaquina {
    /// A pessoa disse não.
    PeloDono,
    /// Não havia tela para mostrar a oferta.
    SemTela,
    /// A máquina só roda no app, e quem recebeu o pedido não é o app.
    SoNoApp,
    /// O pacote que a pessoa tem não é o que o servidor ligou.
    PacoteDiferente,
    /// A oferta não passou na validação local.
    OfertaInvalida,
}

/// Por que uma máquina parou.
///
/// **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotivoDaParadaDeMaquina {
    /// A pessoa a parou.
    PeloDono,
    /// A pessoa revogou o aceite.
    Revogada,
    /// O app fechou.
    AppFechou,
    /// A metade que roda no jogo falhou.
    MetadeFalhou,
}

/// Por que o servidor dispensou uma máquina.
///
/// **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotivoDaDispensa {
    /// O MOD foi desligado.
    ModDesligado,
    /// Outra pessoa passou a operar a máquina.
    OutroPedido,
    /// O aceite não veio.
    SemAceite,
}

/// O nome em kebab-case de uma recusa: o que os logs e a casca mostram.
#[must_use]
pub const fn nome_da_recusa(motivo: &MotivoDaRecusaDeMaquina) -> &'static str {
    match motivo {
        MotivoDaRecusaDeMaquina::PeloDono => "pelo-dono",
        MotivoDaRecusaDeMaquina::SemTela => "sem-tela",
        MotivoDaRecusaDeMaquina::SoNoApp => "so-no-app",
        MotivoDaRecusaDeMaquina::PacoteDiferente => "pacote-diferente",
        MotivoDaRecusaDeMaquina::OfertaInvalida => "oferta-invalida",
    }
}

/// O nome em kebab-case de uma parada.
#[must_use]
pub const fn nome_da_parada(motivo: &MotivoDaParadaDeMaquina) -> &'static str {
    match motivo {
        MotivoDaParadaDeMaquina::PeloDono => "pelo-dono",
        MotivoDaParadaDeMaquina::Revogada => "revogada",
        MotivoDaParadaDeMaquina::AppFechou => "app-fechou",
        MotivoDaParadaDeMaquina::MetadeFalhou => "metade-falhou",
    }
}

/// O nome em kebab-case de uma dispensa.
#[must_use]
pub const fn nome_da_dispensa(motivo: &MotivoDaDispensa) -> &'static str {
    match motivo {
        MotivoDaDispensa::ModDesligado => "mod-desligado",
        MotivoDaDispensa::OutroPedido => "outro-pedido",
        MotivoDaDispensa::SemAceite => "sem-aceite",
    }
}

/// Os checks de forma dos verbos da máquina, compartilhados pelos dois
/// `Validate` de `control.rs`.
pub(crate) fn check_maquina(mod_id: &str, servidor: &str) -> Result<(), ControlError> {
    check_bounds(
        "mod_id",
        mod_id.len(),
        crate::control::MAX_ID_DO_MOD_NOS_PEDIDOS,
    )?;
    if !crate::cargos::id_de_mod_valido(mod_id) {
        return Err(fora("mod_id"));
    }
    if !servidor_valido(servidor) {
        return Err(fora("servidor"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH_A: &str = "aa00000000000000000000000000000000000000000000000000000000000000";
    const HASH_B: &str = "bb00000000000000000000000000000000000000000000000000000000000000";

    fn sha(c: char, n: usize) -> String {
        c.to_string().repeat(n)
    }

    fn oferta_de_exemplo() -> OfertaDeMaquina {
        OfertaDeMaquina {
            titulo: "Minecraft 1.21".into(),
            receita: "minecraft-java".into(),
            versao_da_receita: "1.0.0".into(),
            versao_do_jogo: "1.21.4".into(),
            tuneis: vec![TunelOferecido {
                nome: "mc-1".into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta: 25565,
            }],
            artefatos: vec![ArtefatoOferecido {
                nome: "server.jar".into(),
                url: "https://piston-data.mojang.com/server.jar".into(),
                tamanho: 51_000_000,
                hash: HashDeArtefato {
                    algoritmo: AlgoritmoDeHash::Sha1,
                    hex: sha('a', 40),
                },
            }],
            pack: Some(HashDeArtefato {
                algoritmo: AlgoritmoDeHash::Sha256,
                hex: sha('b', 64),
            }),
            origens: vec!["piston-data.mojang.com".into()],
            dominios_do_jogo: vec!["sessionserver.mojang.com".into()],
            rodar: Some(ComandoOferecido {
                comando: "java".into(),
                argumentos: vec!["-jar".into(), "server.jar".into(), "nogui".into()],
            }),
            disco_maximo: 4 * 1024 * 1024 * 1024,
            termos: vec![TermoOferecido {
                titulo: "EULA".into(),
                url: "https://aka.ms/MinecraftEULA".into(),
            }],
        }
    }

    #[test]
    fn a_oferta_de_exemplo_e_valida() {
        assert!(oferta_de_exemplo().validate().is_ok());
    }

    #[test]
    fn o_resumo_muda_com_cada_coisa_mostrada() {
        let base = oferta_de_exemplo();
        let r = resumo_da_oferta("x/jogo", HASH_A, "mc-1", &base);
        assert_eq!(r, resumo_da_oferta("x/jogo", HASH_A, "mc-1", &base.clone()));

        type Mexe = fn(&mut OfertaDeMaquina);
        let casos: [(&str, Mexe); 16] = [
            ("titulo", |o| o.titulo.push('x')),
            ("receita", |o| o.receita.push('x')),
            ("versao_da_receita", |o| o.versao_da_receita.push('x')),
            ("versao_do_jogo", |o| o.versao_do_jogo.push('x')),
            ("tuneis.porta", |o| o.tuneis[0].porta = 25566),
            ("tuneis.protocolo", |o| {
                o.tuneis[0].protocolo = ProtocoloDeTunel::Udp;
            }),
            ("artefatos.hash", |o| o.artefatos[0].hash.hex = sha('c', 40)),
            ("artefatos.url", |o| o.artefatos[0].url.push('x')),
            ("pack", |o| o.pack = None),
            ("origens", |o| o.origens.push("outra.exemplo".into())),
            ("dominios_do_jogo", |o| o.dominios_do_jogo.clear()),
            ("rodar.argumento", |o| {
                if let Some(r) = o.rodar.as_mut() {
                    r.argumentos[2] = "gui".into();
                }
            }),
            ("rodar", |o| o.rodar = None),
            ("disco_maximo", |o| o.disco_maximo += 1),
            ("termos", |o| o.termos[0].url.push('x')),
            ("termos.vazios", |o| o.termos.clear()),
        ];
        for (nome, mexe) in casos {
            let mut o = base.clone();
            mexe(&mut o);
            assert_ne!(
                r,
                resumo_da_oferta("x/jogo", HASH_A, "mc-1", &o),
                "mexer em {nome} não mudou o resumo"
            );
        }
        assert_ne!(
            r,
            resumo_da_oferta("x/jogo", HASH_B, "mc-1", &base),
            "hash do pacote"
        );
        assert_ne!(
            r,
            resumo_da_oferta("y/jogo", HASH_A, "mc-1", &base),
            "mod_id"
        );
        assert_ne!(
            r,
            resumo_da_oferta("x/jogo", HASH_A, "mc-2", &base),
            "servidor"
        );
    }

    #[test]
    fn o_resumo_em_hex_tem_64_caracteres_minusculos() {
        let r = resumo_da_oferta("x/jogo", HASH_A, "mc-1", &oferta_de_exemplo());
        let hex = resumo_em_hex(&r);
        assert!(crate::mods::e_hash_de_conteudo(&hex), "{hex}");
    }

    #[test]
    fn a_oferta_fora_dos_tetos_e_recusada() {
        type Mexe = fn(&mut OfertaDeMaquina);
        let casos: Vec<(&str, Mexe)> = vec![
            ("titulo vazio", |o| o.titulo.clear()),
            ("titulo longo", |o| o.titulo = "t".repeat(MAX_TITULO + 1)),
            ("receita longa", |o| o.receita = "r".repeat(MAX_TEXTO + 1)),
            ("versao da receita longa", |o| {
                o.versao_da_receita = "v".repeat(MAX_TEXTO + 1);
            }),
            ("versao do jogo longa", |o| {
                o.versao_do_jogo = "v".repeat(MAX_TEXTO + 1);
            }),
            ("tuneis demais", |o| {
                o.tuneis = (0..=MAX_TUNEIS_NA_OFERTA)
                    .map(|i| TunelOferecido {
                        nome: format!("t{i}"),
                        protocolo: ProtocoloDeTunel::Tcp,
                        porta: 1000,
                    })
                    .collect();
            }),
            ("tunel com nome ruim", |o| o.tuneis[0].nome = "MC_1".into()),
            ("tunel na porta zero", |o| o.tuneis[0].porta = 0),
            ("tunel repetido", |o| {
                let t = o.tuneis[0].clone();
                o.tuneis.push(t);
            }),
            ("artefatos demais", |o| {
                o.artefatos = vec![o.artefatos[0].clone(); MAX_ARTEFATOS + 1];
            }),
            ("url sem https", |o| {
                o.artefatos[0].url = "http://exemplo.com/a.jar".into();
            }),
            ("url longa", |o| {
                o.artefatos[0].url = format!("https://{}", "a".repeat(MAX_URL));
            }),
            ("artefato sem tamanho", |o| o.artefatos[0].tamanho = 0),
            ("artefato maior que o disco", |o| {
                o.artefatos[0].tamanho = MAX_DISCO + 1;
            }),
            ("hex de sha1 com 64 caracteres", |o| {
                o.artefatos[0].hash.hex = sha('a', 64);
            }),
            ("hex em maiusculas", |o| {
                o.artefatos[0].hash.hex = sha('A', 40);
            }),
            ("hex do pack curto", |o| {
                o.pack = Some(HashDeArtefato {
                    algoritmo: AlgoritmoDeHash::Sha512,
                    hex: sha('b', 64),
                });
            }),
            ("origens demais", |o| {
                o.origens = (0..=MAX_ORIGENS).map(|i| format!("h{i}.exemplo")).collect();
            }),
            ("origem com esquema", |o| {
                o.origens[0] = "https://a.com".into()
            }),
            ("origem com barra", |o| o.origens[0] = "a.com/x".into()),
            ("dominios demais", |o| {
                o.dominios_do_jogo = (0..=MAX_DOMINIOS_DO_JOGO)
                    .map(|i| format!("h{i}.exemplo"))
                    .collect();
            }),
            ("dominio vazio", |o| o.dominios_do_jogo[0].clear()),
            ("comando vazio", |o| {
                if let Some(r) = o.rodar.as_mut() {
                    r.comando.clear();
                }
            }),
            ("argumentos demais", |o| {
                if let Some(r) = o.rodar.as_mut() {
                    r.argumentos = vec!["a".into(); MAX_ARGUMENTOS + 1];
                }
            }),
            ("argumento longo", |o| {
                if let Some(r) = o.rodar.as_mut() {
                    r.argumentos[0] = "a".repeat(MAX_TEXTO + 1);
                }
            }),
            ("disco acima de 64 GiB", |o| o.disco_maximo = MAX_DISCO + 1),
            ("termos demais", |o| {
                o.termos = vec![o.termos[0].clone(); MAX_TERMOS + 1];
            }),
            ("termo sem https", |o| o.termos[0].url = "ftp://a/b".into()),
        ];
        for (nome, mexe) in casos {
            let mut o = oferta_de_exemplo();
            mexe(&mut o);
            assert!(o.validate().is_err(), "{nome} passou");
        }
    }

    #[test]
    fn nos_tetos_a_oferta_passa() {
        let mut o = oferta_de_exemplo();
        o.titulo = "t".repeat(MAX_TITULO);
        o.disco_maximo = MAX_DISCO;
        o.artefatos[0].url = format!("https://{}", "a".repeat(MAX_URL - 8));
        o.origens = (0..MAX_ORIGENS).map(|i| format!("h{i}.exemplo")).collect();
        assert!(o.validate().is_ok());
    }

    #[test]
    fn o_servidor_segue_a_regua_do_nome_de_tunel() {
        assert!(servidor_valido("mc-1"));
        assert!(servidor_valido(&"a".repeat(MAX_SERVIDOR)));
        for ruim in ["", "MC_1", "-a", "a-", "a b", &"a".repeat(MAX_SERVIDOR + 1)] {
            assert!(!servidor_valido(ruim), "{ruim:?}");
        }
    }

    #[test]
    fn os_nomes_dos_motivos_sao_kebab() {
        use MotivoDaDispensa::*;
        use MotivoDaParadaDeMaquina as P;
        use MotivoDaRecusaDeMaquina as R;
        assert_eq!(
            [
                R::PeloDono,
                R::SemTela,
                R::SoNoApp,
                R::PacoteDiferente,
                R::OfertaInvalida
            ]
            .iter()
            .map(nome_da_recusa)
            .collect::<Vec<_>>(),
            [
                "pelo-dono",
                "sem-tela",
                "so-no-app",
                "pacote-diferente",
                "oferta-invalida"
            ]
        );
        assert_eq!(
            [P::PeloDono, P::Revogada, P::AppFechou, P::MetadeFalhou]
                .iter()
                .map(nome_da_parada)
                .collect::<Vec<_>>(),
            ["pelo-dono", "revogada", "app-fechou", "metade-falhou"]
        );
        assert_eq!(
            [ModDesligado, OutroPedido, SemAceite]
                .iter()
                .map(nome_da_dispensa)
                .collect::<Vec<_>>(),
            ["mod-desligado", "outro-pedido", "sem-aceite"]
        );
    }
}
