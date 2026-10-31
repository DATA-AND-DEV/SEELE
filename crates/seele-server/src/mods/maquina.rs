//! `mundo.maquinas`: a metade de servidor de um MOD chama a máquina de jogo de
//! uma pessoa e avisa a máquina aceita (API 8, parte 3).
//!
//! # Declarar dentro, despejar fora
//!
//! Como `mundo.avisar` e `mundo.tema`, as duas chamadas **declaram**: o que o
//! MOD pediu fica com o anfitrião e só sai se a chamada terminar e o quintal
//! gravar. O JavaScript roda num `spawn_blocking` ou na thread `seele-mods`,
//! longe do Tokio, e o livro das máquinas é um `std::sync::Mutex` cuja ordem
//! com o banco ([`crate::server::Server::maquinas`]) um MOD não tem como
//! respeitar de dentro. Por isso nada aqui toca o livro durante a chamada:
//! [`despejar`] leva o declarado ao livro depois, com o banco lido e solto
//! antes.
//!
//! # O que valida na hora, e o que só o despejo sabe
//!
//! A forma — o servidor de jogo, `quem`, a oferta inteira — é conferida na
//! chamada, e a inválida **lança** com o nome da regra: o MOD lê o erro onde
//! errou. Se `quem` está aqui no 12, e se há máquina aceita para um aviso, só o
//! despejo sabe; e o que ele recusa volta ao MOD como momento
//! (`MaquinaRecusou { motivo: "nao-esta-aqui" }`) ou ao `seele.log`
//! (`sem-maquina`), nunca em silêncio.

use std::sync::Arc;

use seele_proto::control::Validate as _;
use seele_proto::ids::PersonId;
use seele_proto::maquina::{
    AlgoritmoDeHash, ArtefatoOferecido, ComandoOferecido, HashDeArtefato, OfertaDeMaquina,
    TermoOferecido, TunelOferecido,
};
use seele_proto::tunel::ProtocoloDeTunel;
use serde_json::{Map, Value};

use super::aviso::{AvisoDeclarado, AvisoQueSai, TETO_DO_AVISO};
use crate::maquinas::{Excesso, MudancaDeMaquina, Pedido};
use crate::server::{Event, Server};

/// Quantos servidores de jogo uma chamada pode pedir. Pedir é mostrar uma tela
/// a alguém; quatro numa volta já é mais do que uma pessoa lê.
pub const MAX_PEDIDOS_DE_MAQUINA_POR_CHAMADA: usize = 4;

/// Quantos servidores de jogo uma chamada pode avisar: o mesmo número de canais
/// de `mundo.avisar` ([`super::aviso::TETO_DE_AVISOS_POR_CHAMADA`]), pela mesma
/// razão — memória do servidor gasta por um laço do MOD.
pub const MAX_AVISOS_A_MAQUINA_POR_CHAMADA: usize = 16;

/// O motivo do `MaquinaRecusou` que o servidor dá quando `quem` não está
/// conectado num 12 (ou não existe): o pedido não teria a quem ser mostrado.
pub const NAO_ESTA_AQUI: &str = "nao-esta-aqui";

/// O que o log diz de um aviso a uma máquina que não há.
pub const SEM_MAQUINA: &str = "sem-maquina";

/// Um `mundo.maquinas.pedir` que passou na forma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoDeclarado {
    /// Quem vai operar a máquina.
    pub quem: PersonId,
    /// O servidor de jogo.
    pub servidor: String,
    /// O que a pessoa vai ver.
    pub oferta: OfertaDeMaquina,
}

/// O que uma chamada declarou em `mundo.maquinas`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declarado {
    /// Os pedidos, o mais novo de cada servidor de jogo.
    pub pedidos: Vec<PedidoDeclarado>,
    /// Os avisos, o mais novo de cada servidor de jogo (`canal` é o servidor).
    pub avisos: Vec<AvisoDeclarado<String>>,
}

impl Declarado {
    /// Nada a despejar.
    #[must_use]
    pub fn vazio(&self) -> bool {
        self.pedidos.is_empty() && self.avisos.is_empty()
    }

    /// Guarda o pedido, o mais novo de cada servidor de jogo.
    ///
    /// # Errors
    ///
    /// `pedidos-de-maquina-demais` além de [`MAX_PEDIDOS_DE_MAQUINA_POR_CHAMADA`].
    pub fn juntar_pedido(&mut self, pedido: PedidoDeclarado) -> Result<(), &'static str> {
        if let Some(mesmo) = self
            .pedidos
            .iter_mut()
            .find(|p| p.servidor == pedido.servidor)
        {
            *mesmo = pedido;
            return Ok(());
        }
        if self.pedidos.len() >= MAX_PEDIDOS_DE_MAQUINA_POR_CHAMADA {
            return Err("pedidos-de-maquina-demais");
        }
        self.pedidos.push(pedido);
        Ok(())
    }

    /// Guarda o aviso, o mais novo de cada servidor de jogo.
    ///
    /// # Errors
    ///
    /// `avisos-a-maquina-demais` além de [`MAX_AVISOS_A_MAQUINA_POR_CHAMADA`].
    pub fn juntar_aviso(&mut self, aviso: AvisoDeclarado<String>) -> Result<(), &'static str> {
        if let Some(mesmo) = self.avisos.iter_mut().find(|a| a.canal == aviso.canal) {
            *mesmo = aviso;
            return Ok(());
        }
        if self.avisos.len() >= MAX_AVISOS_A_MAQUINA_POR_CHAMADA {
            return Err("avisos-a-maquina-demais");
        }
        self.avisos.push(aviso);
        Ok(())
    }
}

/// O valor que o MOD passou, como JSON. `None` quando não é um objeto.
fn objeto_da_chamada<'js>(
    ctx: &rquickjs::Ctx<'js>,
    valor: rquickjs::Value<'js>,
) -> rquickjs::Result<Option<Map<String, Value>>> {
    if valor.type_of() != rquickjs::Type::Object {
        return Ok(None);
    }
    let Some(texto) = ctx.json_stringify(valor)? else {
        return Ok(None);
    };
    Ok(match serde_json::from_str(&texto.to_string()?) {
        Ok(Value::Object(objeto)) => Some(objeto),
        _ => None,
    })
}

fn servidor_de(objeto: &Map<String, Value>) -> Result<String, String> {
    match objeto.get("servidor").and_then(Value::as_str) {
        Some(s) if seele_proto::maquina::servidor_valido(s) => Ok(s.to_owned()),
        _ => Err("servidor-invalido".to_owned()),
    }
}

/// `quem`: o id numérico da pessoa, como número ou como o texto de
/// `contexto.person` (que é texto, para não perder precisão em JavaScript).
fn quem_de(objeto: &Map<String, Value>) -> Result<PersonId, String> {
    let quem = match objeto.get("quem") {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => {
            s.parse().ok()
        }
        _ => None,
    };
    quem.filter(|n| *n != 0)
        .map(PersonId)
        .ok_or_else(|| "quem-invalido".to_owned())
}

/// O argumento de `mundo.maquinas.pedir`, ou o nome da regra que ele quebrou.
///
/// # Errors
///
/// Só o que o próprio MOD lançar ao ser serializado (um getter, um `toJSON`):
/// é a exceção dele, e ela volta a ele.
pub fn pedido_da_chamada<'js>(
    ctx: &rquickjs::Ctx<'js>,
    valor: rquickjs::Value<'js>,
) -> rquickjs::Result<Result<PedidoDeclarado, String>> {
    let Some(objeto) = objeto_da_chamada(ctx, valor)? else {
        return Ok(Err("pedido-invalido".to_owned()));
    };
    Ok((|| {
        let servidor = servidor_de(&objeto)?;
        let quem = quem_de(&objeto)?;
        let oferta = match objeto.get("oferta") {
            Some(Value::Object(oferta)) => oferta_do_json(oferta),
            _ => Err("oferta".to_owned()),
        }
        .map_err(|campo| format!("oferta-invalida:{campo}"))?;
        Ok(PedidoDeclarado {
            quem,
            servidor,
            oferta,
        })
    })())
}

/// O argumento de `mundo.maquinas.avisar` (`{ servidor, carga }`), ou o nome
/// da recusa: `servidor-invalido`, `aviso-invalido` (sem carga, ou uma que o
/// JSON não escreve) e `aviso-grande-demais` (acima de [`TETO_DO_AVISO`]).
///
/// # Errors
///
/// As de [`pedido_da_chamada`].
pub fn aviso_da_chamada<'js>(
    ctx: &rquickjs::Ctx<'js>,
    valor: rquickjs::Value<'js>,
) -> rquickjs::Result<Result<AvisoDeclarado<String>, &'static str>> {
    let Some(objeto) = valor
        .as_object()
        .filter(|_| valor.type_of() == rquickjs::Type::Object)
    else {
        return Ok(Err("aviso-invalido"));
    };
    let servidor_js: rquickjs::Value<'js> = objeto.get("servidor")?;
    let Some(servidor) = servidor_js
        .as_string()
        .map(rquickjs::String::to_string)
        .transpose()?
        .filter(|s| seele_proto::maquina::servidor_valido(s))
    else {
        return Ok(Err("servidor-invalido"));
    };
    let carga_js: rquickjs::Value<'js> = objeto.get("carga")?;
    let Some(texto) = ctx.json_stringify(carga_js)? else {
        return Ok(Err("aviso-invalido"));
    };
    let carga = texto.to_string()?;
    if carga.len() > TETO_DO_AVISO {
        return Ok(Err("aviso-grande-demais"));
    }
    Ok(Ok(AvisoDeclarado {
        canal: servidor,
        carga,
    }))
}

/// Recusa uma chave que o tipo não tem: `tunels` por `tuneis` viraria uma
/// oferta sem túnel nenhum, e a pessoa aceitaria outra coisa que o autor quis.
fn so_as_chaves(objeto: &Map<String, Value>, chaves: &[&str]) -> Result<(), String> {
    match objeto.keys().find(|k| !chaves.contains(&k.as_str())) {
        Some(estranha) => Err(estranha.clone()),
        None => Ok(()),
    }
}

/// Um texto; ausente ou `null` é vazio (e a validação do fio decide se vazio
/// vale ali).
fn texto(objeto: &Map<String, Value>, chave: &str, campo: &str) -> Result<String, String> {
    match objeto.get(chave) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(campo.to_owned()),
    }
}

/// Um inteiro sem sinal; ausente ou `null` é zero.
fn numero(objeto: &Map<String, Value>, chave: &str, campo: &str) -> Result<u64, String> {
    match objeto.get(chave) {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(n)) => n.as_u64().ok_or_else(|| campo.to_owned()),
        Some(_) => Err(campo.to_owned()),
    }
}

/// Uma lista; ausente ou `null` é vazia.
fn lista<'a>(
    objeto: &'a Map<String, Value>,
    chave: &str,
    campo: &str,
) -> Result<&'a [Value], String> {
    match objeto.get(chave) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(itens)) => Ok(itens),
        Some(_) => Err(campo.to_owned()),
    }
}

fn objeto<'a>(valor: &'a Value, campo: &str) -> Result<&'a Map<String, Value>, String> {
    valor.as_object().ok_or_else(|| campo.to_owned())
}

fn textos(objeto: &Map<String, Value>, chave: &str, campo: &str) -> Result<Vec<String>, String> {
    lista(objeto, chave, campo)?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| campo.to_owned())
        })
        .collect()
}

fn hash(valor: &Value, campo: &str) -> Result<HashDeArtefato, String> {
    let h = objeto(valor, campo)?;
    so_as_chaves(h, &["algoritmo", "hex"]).map_err(|_| campo.to_owned())?;
    let algoritmo = match h.get("algoritmo").and_then(Value::as_str) {
        Some("sha1") => AlgoritmoDeHash::Sha1,
        Some("sha256") => AlgoritmoDeHash::Sha256,
        Some("sha512") => AlgoritmoDeHash::Sha512,
        _ => return Err(campo.to_owned()),
    };
    Ok(HashDeArtefato {
        algoritmo,
        hex: texto(h, "hex", campo)?,
    })
}

/// A oferta como o MOD a escreve — os nomes de campo de [`OfertaDeMaquina`],
/// o protocolo (`tcp`, `udp`) e o algoritmo (`sha1`, `sha256`, `sha512`) em
/// minúsculas —, já validada como o fio a validaria.
///
/// # Errors
///
/// O campo que não passou: o nome da chave quando é a forma (uma chave que o
/// tipo não tem, um número onde vai texto), e o nome que
/// [`OfertaDeMaquina::validate`] dá quando é a regra.
pub fn oferta_do_json(o: &Map<String, Value>) -> Result<OfertaDeMaquina, String> {
    so_as_chaves(
        o,
        &[
            "titulo",
            "receita",
            "versao_da_receita",
            "versao_do_jogo",
            "tuneis",
            "artefatos",
            "pack",
            "origens",
            "dominios_do_jogo",
            "rodar",
            "disco_maximo",
            "termos",
        ],
    )?;
    let tuneis = lista(o, "tuneis", "tuneis")?
        .iter()
        .map(|t| {
            let t = objeto(t, "tuneis")?;
            so_as_chaves(t, &["nome", "protocolo", "porta"]).map_err(|_| "tuneis".to_owned())?;
            let protocolo = match t.get("protocolo").and_then(Value::as_str) {
                Some("tcp") => ProtocoloDeTunel::Tcp,
                Some("udp") => ProtocoloDeTunel::Udp,
                _ => return Err("tuneis".to_owned()),
            };
            Ok(TunelOferecido {
                nome: texto(t, "nome", "tuneis")?,
                protocolo,
                porta: u16::try_from(numero(t, "porta", "tuneis")?)
                    .map_err(|_| "tuneis".to_owned())?,
            })
        })
        .collect::<Result<_, String>>()?;
    let artefatos = lista(o, "artefatos", "artefatos")?
        .iter()
        .map(|a| {
            let a = objeto(a, "artefatos")?;
            so_as_chaves(a, &["nome", "url", "tamanho", "hash", "plataformas"])?;
            // Opcional (API 8 parte 4): sem a chave, o artefato serve em todo
            // sistema, como antes de a marca existir.
            let plataformas = match a.get("plataformas") {
                None | Some(Value::Null) => Vec::new(),
                Some(_) => textos(a, "plataformas", "artefato_plataformas")?,
            };
            Ok(ArtefatoOferecido {
                nome: texto(a, "nome", "artefato_nome")?,
                url: texto(a, "url", "artefato_url")?,
                tamanho: numero(a, "tamanho", "artefato_tamanho")?,
                hash: hash(a.get("hash").unwrap_or(&Value::Null), "artefato_hash")?,
                plataformas,
            })
        })
        .collect::<Result<_, String>>()?;
    let pack = match o.get("pack") {
        None | Some(Value::Null) => None,
        Some(h) => Some(hash(h, "pack")?),
    };
    let rodar = match o.get("rodar") {
        None | Some(Value::Null) => None,
        Some(r) => {
            let r = objeto(r, "rodar")?;
            so_as_chaves(r, &["comando", "argumentos"])?;
            Some(ComandoOferecido {
                comando: texto(r, "comando", "comando")?,
                argumentos: textos(r, "argumentos", "argumentos")?,
            })
        }
    };
    let termos = lista(o, "termos", "termos")?
        .iter()
        .map(|t| {
            let t = objeto(t, "termos")?;
            so_as_chaves(t, &["titulo", "url"])?;
            Ok(TermoOferecido {
                titulo: texto(t, "titulo", "termo_titulo")?,
                url: texto(t, "url", "termo_url")?,
            })
        })
        .collect::<Result<_, String>>()?;
    let oferta = OfertaDeMaquina {
        titulo: texto(o, "titulo", "titulo")?,
        receita: texto(o, "receita", "receita")?,
        versao_da_receita: texto(o, "versao_da_receita", "versao_da_receita")?,
        versao_do_jogo: texto(o, "versao_do_jogo", "versao_do_jogo")?,
        tuneis,
        artefatos,
        pack,
        origens: textos(o, "origens", "origens")?,
        dominios_do_jogo: textos(o, "dominios_do_jogo", "dominios_do_jogo")?,
        rodar,
        disco_maximo: numero(o, "disco_maximo", "disco_maximo")?,
        termos,
    };
    oferta.validate().map_err(|erro| match erro {
        seele_proto::control::ControlError::FieldTooLong { field, .. }
        | seele_proto::control::ControlError::FieldOutOfRange { field } => field.to_owned(),
        outro => outro.to_string(),
    })?;
    Ok(oferta)
}

/// Por quanto tempo uma recusa do servidor já dita ao MOD não volta a ele como
/// momento quando o pedido repetido vem **de um momento**.
///
/// Sem isto, um `aoAcontecer` que pede de novo ao ler `MaquinaRecusou` (o
/// tentar-de-novo natural para `nao-esta-aqui`) se alimenta sozinho: o despejo
/// recusa, a recusa vira momento, o momento pede de novo — a thread dos MODs
/// gira, os vizinhos perdem momentos e o log enche. Cinco segundos é mais que
/// uma volta do barramento e menos que uma pessoa leva para entrar.
pub const JANELA_DA_RECUSA_REPETIDA: std::time::Duration = std::time::Duration::from_secs(5);

/// De onde veio a chamada cujo declarado se despeja.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origem {
    /// Um `aoPedir`: a pessoa que pediu (vai em `pedido_por`). Toda recusa do
    /// servidor volta ao MOD como momento.
    Pedido(PersonId),
    /// Um `aoAcontecer`: ninguém pediu (`pedido_por` é `PersonId(0)`, que o app
    /// mostra como «alguém deste servidor»). Uma recusa igual a uma já dita
    /// dentro de [`JANELA_DA_RECUSA_REPETIDA`] não volta como momento: fica no
    /// log e em [`Server::recusas_juntadas`].
    Momento,
}

impl Origem {
    const fn pedido_por(self) -> PersonId {
        match self {
            Self::Pedido(quem) => quem,
            Self::Momento => PersonId(0),
        }
    }
}

/// O nome com que um teto do livro volta ao MOD em `MaquinaRecusou`.
const fn nome_do_excesso(excesso: Excesso) -> &'static str {
    match excesso {
        Excesso::PendentesDemais => "pendentes-demais",
        Excesso::MaquinasDemais => "maquinas-demais",
    }
}

/// Leva ao livro o que a chamada de `mod_id` declarou em `mundo.maquinas`,
/// depois de ela terminar e de o quintal gravar.
///
/// `origem` diz quem pediu ([`Origem`]) e se uma recusa repetida volta ao MOD.
///
/// A ordem dos cadeados é a de [`Server::maquinas`]: o banco é lido e solto
/// antes, e o livro nunca é tomado com o avisador na mão a partir daqui (o
/// avisador toma o livro ao soltar, em [`entregar_aviso`]).
pub async fn despejar(server: &Arc<Server>, mod_id: &str, origem: Origem, declarado: Declarado) {
    let pedido_por = origem.pedido_por();
    if declarado.vazio() {
        return;
    }
    // O hash é o do pacote ligado **agora**: é sobre ele que o resumo do
    // aceite é feito, e a vigia do conjunto dispensa o que for de outro.
    let hash = {
        let banco = server.persistence.lock().await;
        crate::persistence::mods::enabled(&banco)
            .map(|ligados| ligados.into_iter().find(|m| m.id == mod_id).map(|m| m.hash))
    };
    let hash = match hash {
        Ok(Some(hash)) => hash,
        Ok(None) => {
            tracing::warn!(mod_id = %mod_id, pedidos = declarado.pedidos.len(), avisos = declarado.avisos.len(), motivo = "mod-desligado", "máquina: o MOD saiu do conjunto durante a chamada; o que ela declarou em mundo.maquinas não sai");
            return;
        }
        Err(erro) => {
            tracing::error!(mod_id = %mod_id, %erro, "máquina: não deu para ler a tabela de MODs; o que a chamada declarou em mundo.maquinas não sai");
            return;
        }
    };

    for pedido in declarado.pedidos {
        let PedidoDeclarado {
            quem,
            servidor,
            oferta,
        } = pedido;
        // Um momento que pede de novo a quem acabou de recusar **sozinho**
        // (o navegador com `SoNoApp`, o núcleo sem tela com `SemTela`) teria a
        // mesma recusa na volta da rede, e o laço seguiria no passo dela. O
        // MOD já leu essa recusa uma vez; o pedido repetido não sai.
        if origem == Origem::Momento
            && server.maquinas().recusa_automatica_recente(
                mod_id,
                &servidor,
                quem,
                std::time::Instant::now(),
                JANELA_DA_RECUSA_REPETIDA,
            )
        {
            let juntadas = server.maquinas().contar_recusa_juntada(mod_id);
            tracing::info!(mod_id = %mod_id, servidor = %servidor, quem = %quem, juntadas, "máquina: um momento pediu de novo a quem acabou de recusar sozinho; o pedido não sai");
            continue;
        }
        if !server.maquinas().pode_ser_chamada(quem) {
            tracing::info!(mod_id = %mod_id, servidor = %servidor, quem = %quem, motivo = NAO_ESTA_AQUI, "máquina: pedido a quem não está conectado num 12; volta ao MOD como recusa");
            recusar_ao_mod(server, origem, mod_id, servidor, quem, NAO_ESTA_AQUI);
            continue;
        }
        // `maquinas::pedir` já diz no log o que fez, inclusive o teto.
        if let Pedido::Recusado(excesso) =
            crate::maquinas::pedir(server, mod_id, &hash, &servidor, quem, pedido_por, &oferta)
        {
            recusar_ao_mod(
                server,
                origem,
                mod_id,
                servidor,
                quem,
                nome_do_excesso(excesso),
            );
        }
    }

    let mut a_mandar = Vec::new();
    for aviso in declarado.avisos {
        if server.maquinas().quem_opera(mod_id, &aviso.canal).is_some() {
            a_mandar.push(aviso);
        } else {
            sem_maquina(server, mod_id, &aviso.canal);
        }
    }
    if a_mandar.is_empty() {
        return;
    }
    // Fraco: o avisador guarda o `sair` numa tarefa que espera a janela abrir,
    // e um `Arc` ali seguraria o servidor de pé depois de ele descer.
    let fraco = Arc::downgrade(server);
    server
        .avisador_de_maquinas
        .avisar(mod_id, a_mandar, move |saindo| {
            if let Some(server) = fraco.upgrade() {
                entregar_aviso(&server, saindo);
            }
        });
}

/// O MOD fica sabendo que o pedido não foi mostrado, pelo momento
/// `MaquinaRecusou` — o mesmo que ele lê quando a pessoa diz não.
///
/// Vinda de um momento, a mesma recusa já dita há menos de
/// [`JANELA_DA_RECUSA_REPETIDA`] não volta: o MOD já sabe, e dizer de novo é o
/// laço que se alimenta sozinho. Vinda de um pedido, sempre volta (e marca a
/// janela): um pedido não nasce de um momento, então não fecha laço.
fn recusar_ao_mod(
    server: &Server,
    origem: Origem,
    mod_id: &str,
    servidor: String,
    quem: PersonId,
    motivo: &'static str,
) {
    let agora = std::time::Instant::now();
    let nova = server.maquinas().marcar_recusa(
        mod_id,
        &servidor,
        quem,
        motivo,
        agora,
        JANELA_DA_RECUSA_REPETIDA,
    );
    if !nova && origem == Origem::Momento {
        let juntadas = server.maquinas().contar_recusa_juntada(mod_id);
        tracing::info!(mod_id = %mod_id, servidor = %servidor, quem = %quem, motivo, juntadas, "máquina: a mesma recusa, pedida de novo por um momento, já foi dita ao MOD há pouco; não volta como momento");
        return;
    }
    let _ = server
        .events
        .send(Event::Maquina(MudancaDeMaquina::Recusou {
            mod_id: mod_id.to_owned(),
            servidor,
            quem,
            motivo,
        }));
}

/// Um aviso sem máquina aceita: não sai, e o MOD não lê erro nenhum (a chamada
/// que o declarou já terminou). Fica no log, com o nome, e no contador do MOD
/// ([`Server::avisos_sem_maquina`]).
fn sem_maquina(server: &Server, mod_id: &str, servidor: &str) {
    let quantos = server.maquinas().contar_aviso_sem_maquina(mod_id);
    tracing::info!(mod_id = %mod_id, servidor = %servidor, motivo = SEM_MAQUINA, quantos, "máquina: aviso a um servidor de jogo sem máquina aceita; não saiu");
}

/// Solta um aviso que passou pela taxa: à máquina aceita **agora**, que pode
/// não ser a de quando ele foi declarado.
fn entregar_aviso(server: &Server, saindo: AvisoQueSai<String>) {
    let operador = server.maquinas().operador(&saindo.mod_id, &saindo.canal);
    match operador {
        Some((para, conexao)) => {
            let _ = server.events.send(Event::AvisoParaMaquina {
                para,
                conexao,
                mod_id: saindo.mod_id,
                servidor: saindo.canal,
                carga: saindo.carga,
            });
        }
        None => sem_maquina(server, &saindo.mod_id, &saindo.canal),
    }
}

impl Server {
    /// Quantos avisos de `mod_id` à máquina não saíram por não haver máquina
    /// aceita (`sem-maquina`), desde que o servidor subiu.
    #[must_use]
    pub fn avisos_sem_maquina(&self, mod_id: &str) -> u64 {
        self.maquinas().avisos_sem_maquina(mod_id)
    }

    /// Quantas recusas de `mod_id`, repetidas por um momento dentro de
    /// [`JANELA_DA_RECUSA_REPETIDA`], não voltaram a ele como momento.
    #[must_use]
    pub fn recusas_juntadas(&self, mod_id: &str) -> u64 {
        self.maquinas().recusas_juntadas(mod_id)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// **Os números e os nomes da `v8.json` são os do código** — os tetos por
    /// chamada em `limites`, e o motivo `nao-esta-aqui` e o `sem-maquina` em
    /// `recusas`. O `check-api` não sabe ler prosa, e um teto mudado aqui
    /// deixaria o guia prometendo o número velho.
    #[test]
    fn a_v8_diz_os_tetos_e_os_nomes_do_codigo() {
        let v8: serde_json::Value =
            serde_json::from_str(include_str!("../../../../api/v8.json")).expect("o v8.json");
        let limites = v8["limites"]["maquinas-por-chamada"]
            .as_str()
            .expect("limites.maquinas-por-chamada");
        assert!(
            limites.contains(&format!(
                "{MAX_PEDIDOS_DE_MAQUINA_POR_CHAMADA} servidores de jogo pedidos"
            )) && limites.contains(&format!("e {MAX_AVISOS_A_MAQUINA_POR_CHAMADA} avisados")),
            "os tetos da v8 não são {MAX_PEDIDOS_DE_MAQUINA_POR_CHAMADA} e \
             {MAX_AVISOS_A_MAQUINA_POR_CHAMADA}: {limites}"
        );
        let recusas = v8["recusas"]["maquinas"].to_string();
        for nome in [
            NAO_ESTA_AQUI,
            SEM_MAQUINA,
            "pedidos-de-maquina-demais",
            "avisos-a-maquina-demais",
            nome_do_excesso(Excesso::PendentesDemais),
            nome_do_excesso(Excesso::MaquinasDemais),
        ] {
            assert!(recusas.contains(nome), "`{nome}` falta em recusas.maquinas");
        }
        let momentos = v8["moments"].to_string();
        for nome in ["MaquinaAceitou", "MaquinaRecusou", "MaquinaParou"] {
            assert!(momentos.contains(nome), "`{nome}` falta em moments");
        }
    }
}
