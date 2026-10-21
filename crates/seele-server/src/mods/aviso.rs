//! O aviso da metade de servidor às janelas do mesmo MOD (rascunho da API 6,
//! §7.3; tarefa 8 do plano da F3).
//!
//! Os três MODs oficiais consultam o servidor em laço porque não havia outro
//! jeito: a MESA a cada 2 s, o PERFIS e o ESTILO a cada 4 s. Com `mundo.avisar`,
//! a metade de servidor diz «mudou», e as janelas pedem o que mudou.
//!
//! # A ordem das coisas
//!
//! 1. Dentro de `aoPedir` ou de `aoAcontecer`, `mundo.avisar` **declara**: o
//!    aviso fica com o anfitrião e só sai se a chamada terminar e o quintal
//!    gravar (pedido da MESA, 09/10). Um aviso nunca fala de uma revisão que não
//!    existe.
//! 2. Quem roda a chamada entrega os avisos ao [`Avisador`] do servidor, que
//!    aplica a taxa por MOD e junta o excedente por canal.
//! 3. O avisador põe no barramento um `Event::ModAviso`, e cada sessão o manda
//!    a quem pode ler o canal.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use seele_proto::ids::ChannelId;
use tokio::time::{Duration, Instant};

/// O maior aviso, em bytes do JSON serializado.
///
/// **A proposta do rascunho, a medir na M3** (tarefa 11): um aviso diz «mudou»,
/// e não carrega o que mudou — quem quer o dado pede. O teto do fio é outro, e
/// maior (`MAX_CARGA_DE_MOD`, 12 KiB): este é o da API, que pode mudar sem
/// tocar no fio.
pub const TETO_DO_AVISO: usize = 1024;

/// Quantos avisos um MOD manda por segundo, em qualquer janela de um segundo.
///
/// A proposta do rascunho, a medir na M3, como [`TETO_DO_AVISO`].
pub const AVISOS_POR_SEGUNDO: usize = 4;

/// Quantos canais diferentes uma chamada pode avisar.
///
/// Dentro de uma chamada, os avisos já se juntam por canal; este teto é para o
/// MOD que avisa mil canais inventados numa volta, que seria memória do servidor
/// gasta por ele.
pub const TETO_DE_AVISOS_POR_CHAMADA: usize = 16;

/// Um aviso que a metade de servidor declarou numa chamada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvisoDeclarado {
    /// O canal do aviso; `None` é o servidor inteiro.
    pub canal: Option<ChannelId>,
    /// A carga, já serializada em JSON.
    pub carga: String,
}

/// O aviso que o MOD passou a `mundo.avisar`, ou o nome da recusa.
///
/// `{ canal?, carga }`: `canal` é um inteiro positivo, ou falta, ou `null`;
/// `carga` é um valor JSON qualquer, e é obrigatória — um aviso sem carga não
/// diz nada que a janela possa conferir.
///
/// # Errors
///
/// Só o que o próprio MOD lançar ao ser lido ou serializado (um getter, um
/// `toJSON`): é a exceção dele, e ela volta a ele.
pub fn aviso_da_chamada<'js>(
    ctx: &rquickjs::Ctx<'js>,
    valor: rquickjs::Value<'js>,
) -> rquickjs::Result<Result<AvisoDeclarado, &'static str>> {
    let Some(objeto) = valor
        .as_object()
        .filter(|_| valor.type_of() == rquickjs::Type::Object)
    else {
        return Ok(Err("aviso-invalido"));
    };
    let canal_js: rquickjs::Value<'js> = objeto.get("canal")?;
    let canal = if canal_js.is_undefined() || canal_js.is_null() {
        None
    } else {
        match canal_js.as_number() {
            Some(n) if n.fract() == 0.0 && n >= 1.0 && n <= f64::from(u32::MAX) => {
                // Inteiro e dentro do `u32`, conferido logo acima.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Some(ChannelId(n as u32))
            }
            _ => return Ok(Err("aviso-invalido")),
        }
    };
    let carga_js: rquickjs::Value<'js> = objeto.get("carga")?;
    let Some(texto) = ctx.json_stringify(carga_js)? else {
        // Sem `carga`, ou uma que o JSON não escreve: uma função, um símbolo,
        // um `toJSON` que devolve `undefined`. O teste do `{ canal: 7 }` sem
        // carga passa por aqui.
        return Ok(Err("aviso-invalido"));
    };
    let carga = texto.to_string()?;
    if carga.len() > TETO_DO_AVISO {
        return Ok(Err("aviso-grande-demais"));
    }
    Ok(Ok(AvisoDeclarado { canal, carga }))
}

/// Guarda o aviso na lista da chamada, **o mais novo de cada canal**.
///
/// Devolve a recusa quando a chamada já avisou canais demais.
pub fn juntar(lista: &mut Vec<AvisoDeclarado>, aviso: AvisoDeclarado) -> Result<(), &'static str> {
    if let Some(mesmo) = lista.iter_mut().find(|outro| outro.canal == aviso.canal) {
        *mesmo = aviso;
        return Ok(());
    }
    if lista.len() >= TETO_DE_AVISOS_POR_CHAMADA {
        return Err("avisos-demais");
    }
    lista.push(aviso);
    Ok(())
}

/// O estado de um MOD no avisador.
#[derive(Default)]
struct DoMod {
    /// Quando saíram os últimos avisos, até [`AVISOS_POR_SEGUNDO`].
    saidas: VecDeque<Instant>,
    /// O mais novo de cada canal que ainda não saiu, e quantos ele substituiu.
    pendentes: BTreeMap<Option<ChannelId>, (u64, String, u64)>,
    /// A ordem de chegada dos pendentes, para o mais antigo sair primeiro.
    proxima_ordem: u64,
    /// Se já há uma tarefa esperando a janela abrir para este MOD.
    esperando: bool,
}

/// **A taxa por MOD, com o excedente junto por canal** (pedido da MESA, 09/10).
///
/// Um MOD manda no máximo [`AVISOS_POR_SEGUNDO`] avisos em qualquer janela de um
/// segundo. O que passa disso **não é descartado**: fica, por canal, só o mais
/// novo, e sai assim que a janela abre. Os do meio são substituídos e contados
/// no `seele.log` quando o mais novo sai. Assim a última revisão sempre chega:
/// numa luta, as rolagens de cinco pessoas passam de 4 por segundo, e um aviso
/// descartado deixaria uma janela parada na revisão velha.
#[derive(Default)]
pub struct Avisador {
    mods: Mutex<BTreeMap<String, DoMod>>,
}

/// Um aviso pronto para o barramento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvisoQueSai {
    /// O MOD que avisa.
    pub mod_id: String,
    /// O canal; `None` é o servidor inteiro.
    pub canal: Option<ChannelId>,
    /// A carga, em JSON.
    pub carga: String,
}

impl Avisador {
    /// Entrega os avisos de uma chamada que terminou.
    ///
    /// `sair` é chamado para cada aviso que sai agora; o que fica para depois é
    /// mandado por uma tarefa que espera a janela abrir, e chama o mesmo `sair`.
    pub fn avisar<F>(
        self: &std::sync::Arc<Self>,
        mod_id: &str,
        avisos: Vec<AvisoDeclarado>,
        sair: F,
    ) where
        F: Fn(AvisoQueSai) + Send + Sync + Clone + 'static,
    {
        if avisos.is_empty() {
            return;
        }
        let precisa_esperar = {
            let Ok(mut mods) = self.mods.lock() else {
                return;
            };
            let do_mod = mods.entry(mod_id.to_owned()).or_default();
            for aviso in avisos {
                // **A vez é do canal, e não do aviso**: o canal que espera há
                // mais tempo sai primeiro, e substituir o aviso dele não o
                // manda para o fim da fila.
                let (ordem, substituidos) = match do_mod.pendentes.get(&aviso.canal) {
                    Some((ordem, _, contados)) => (*ordem, contados + 1),
                    None => {
                        let ordem = do_mod.proxima_ordem;
                        do_mod.proxima_ordem += 1;
                        (ordem, 0)
                    }
                };
                do_mod
                    .pendentes
                    .insert(aviso.canal, (ordem, aviso.carga, substituidos));
            }
            Self::soltar_o_que_cabe(mod_id, do_mod, Instant::now(), &sair);
            let precisa = !do_mod.pendentes.is_empty() && !do_mod.esperando;
            if precisa {
                do_mod.esperando = true;
            }
            precisa
        };
        if precisa_esperar {
            let avisador = std::sync::Arc::clone(self);
            let mod_id = mod_id.to_owned();
            tokio::spawn(async move { avisador.esperar_e_soltar(&mod_id, sair).await });
        }
    }

    /// Solta os pendentes até a taxa deixar, do mais antigo para o mais novo.
    fn soltar_o_que_cabe<F: Fn(AvisoQueSai)>(
        mod_id: &str,
        do_mod: &mut DoMod,
        agora: Instant,
        sair: &F,
    ) {
        while let Some(&mais_antiga) = do_mod.saidas.front() {
            if agora.duration_since(mais_antiga) >= Duration::from_secs(1) {
                do_mod.saidas.pop_front();
            } else {
                break;
            }
        }
        while do_mod.saidas.len() < AVISOS_POR_SEGUNDO {
            let Some(canal) = do_mod
                .pendentes
                .iter()
                .min_by_key(|(_, (ordem, _, _))| *ordem)
                .map(|(canal, _)| *canal)
            else {
                break;
            };
            let Some((_, carga, substituidos)) = do_mod.pendentes.remove(&canal) else {
                break;
            };
            if substituidos > 0 {
                tracing::info!(
                    mod_id = %mod_id,
                    canal = ?canal.map(|c| c.0),
                    substituidos,
                    "avisos do MOD juntados pela taxa: saiu o mais novo"
                );
            }
            do_mod.saidas.push_back(agora);
            sair(AvisoQueSai {
                mod_id: mod_id.to_owned(),
                canal,
                carga,
            });
        }
    }

    /// Espera a janela abrir e solta o que couber, até não sobrar pendente.
    async fn esperar_e_soltar<F: Fn(AvisoQueSai)>(&self, mod_id: &str, sair: F) {
        loop {
            let acorda = {
                let Ok(mods) = self.mods.lock() else {
                    return;
                };
                let Some(do_mod) = mods.get(mod_id) else {
                    return;
                };
                do_mod
                    .saidas
                    .front()
                    .map_or_else(Instant::now, |mais_antiga| {
                        *mais_antiga + Duration::from_secs(1)
                    })
            };
            tokio::time::sleep_until(acorda).await;
            let Ok(mut mods) = self.mods.lock() else {
                return;
            };
            let Some(do_mod) = mods.get_mut(mod_id) else {
                return;
            };
            Self::soltar_o_que_cabe(mod_id, do_mod, Instant::now(), &sair);
            if do_mod.pendentes.is_empty() {
                do_mod.esperando = false;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::sync::{Arc, Mutex as Trava};

    /// **Dezesseis canais por chamada, e o seguinte é `avisos-demais`**
    /// (`sentido.mundo.avisar`; a auditoria do `v6.json`, tabela 2: nada citava
    /// o teto nem o nome, e tirar a conferência de [`juntar`] passava). O número
    /// é o que o texto diz, lido dele. O mesmo canal de novo não conta: troca o
    /// aviso de antes, e o mais novo fica.
    #[test]
    fn uma_chamada_avisa_ate_o_teto_de_canais_e_o_seguinte_e_avisos_demais() {
        let v6: serde_json::Value =
            serde_json::from_str(include_str!("../../../../api/v6.json")).expect("o v6.json");
        let texto = v6["sentido"]["mundo.avisar"]
            .as_str()
            .expect("o texto do avisar");
        assert!(
            texto.contains(&format!("{TETO_DE_AVISOS_POR_CHAMADA} canais por chamada"))
                && texto.contains("avisos-demais"),
            "o teto de {TETO_DE_AVISOS_POR_CHAMADA} canais por chamada, ou o nome da recusa, não é o \
             que o v6.json diz: {texto}"
        );
        let aviso = |canal: u32, carga: &str| AvisoDeclarado {
            canal: Some(ChannelId(canal)),
            carga: carga.to_owned(),
        };
        let teto = u32::try_from(TETO_DE_AVISOS_POR_CHAMADA).expect("o teto cabe");
        let mut lista = Vec::new();
        for canal in 1..=teto {
            assert_eq!(
                juntar(&mut lista, aviso(canal, "1")),
                Ok(()),
                "o canal {canal} cabia"
            );
        }
        assert_eq!(
            juntar(&mut lista, aviso(3, "2")),
            Ok(()),
            "o mesmo canal de novo foi contado como um canal a mais"
        );
        assert_eq!(
            lista
                .iter()
                .find(|a| a.canal == Some(ChannelId(3)))
                .map(|a| a.carga.as_str()),
            Some("2"),
            "o mesmo canal de novo não ficou com o mais novo: {lista:?}"
        );
        assert_eq!(
            juntar(&mut lista, aviso(teto + 1, "1")),
            Err("avisos-demais"),
            "o canal além do teto não foi recusado pelo nome"
        );
        assert_eq!(
            lista.len(),
            TETO_DE_AVISOS_POR_CHAMADA,
            "o recusado entrou na lista"
        );
    }

    /// Um avisador e o que ele já soltou, na ordem.
    fn avisador() -> (Arc<Avisador>, Arc<Trava<Vec<AvisoQueSai>>>) {
        (
            Arc::new(Avisador::default()),
            Arc::new(Trava::new(Vec::new())),
        )
    }

    fn avisar(
        avisador: &Arc<Avisador>,
        saiu: &Arc<Trava<Vec<AvisoQueSai>>>,
        mod_id: &str,
        canal: u32,
        carga: &str,
    ) {
        let saiu = Arc::clone(saiu);
        avisador.avisar(
            mod_id,
            vec![AvisoDeclarado {
                canal: Some(ChannelId(canal)),
                carga: carga.to_owned(),
            }],
            move |aviso| saiu.lock().unwrap().push(aviso),
        );
    }

    fn cargas(saiu: &Arc<Trava<Vec<AvisoQueSai>>>) -> Vec<String> {
        saiu.lock()
            .unwrap()
            .iter()
            .map(|a| a.carga.clone())
            .collect()
    }

    /// Deixa a tarefa que espera a janela rodar até onde o relógio parado deixa.
    async fn passar(tempo: Duration) {
        tokio::time::advance(tempo).await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    }

    /// **Os avisos do mesmo canal acima da taxa saem como quatro, e o último é
    /// o mais novo** (pedido da MESA, 09/10). Os do meio são substituídos, e não
    /// descartados no fim da fila: uma janela parada na revisão velha é o
    /// defeito que esta regra existe para não ter.
    #[tokio::test(start_paused = true)]
    async fn quatro_saem_na_hora_e_o_excedente_sai_como_o_mais_novo() {
        let (avisador, saiu) = avisador();
        for n in 1..=10 {
            avisar(&avisador, &saiu, "seele/mesa", 7, &n.to_string());
        }
        assert_eq!(cargas(&saiu), ["1", "2", "3", "4"], "passou da taxa");

        // Aos 600 ms, a janela de um segundo ainda tem os quatro: o décimo
        // primeiro espera, junto com o décimo, e fica no lugar dele.
        passar(Duration::from_millis(600)).await;
        avisar(&avisador, &saiu, "seele/mesa", 7, "11");
        assert_eq!(
            cargas(&saiu).len(),
            4,
            "um aviso saiu com a janela de um segundo cheia"
        );

        passar(Duration::from_millis(399)).await;
        assert_eq!(
            cargas(&saiu).len(),
            4,
            "o excedente saiu antes de a janela abrir"
        );

        passar(Duration::from_millis(2)).await;
        assert_eq!(
            cargas(&saiu),
            ["1", "2", "3", "4", "11"],
            "o excedente não saiu como o mais novo"
        );

        passar(Duration::from_secs(5)).await;
        assert_eq!(
            cargas(&saiu).len(),
            5,
            "um aviso substituído saiu mesmo assim"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cada_canal_fica_com_o_seu_mais_novo() {
        let (avisador, saiu) = avisador();
        for n in 1..=4 {
            avisar(&avisador, &saiu, "seele/mesa", 1, &format!("um-{n}"));
        }
        avisar(&avisador, &saiu, "seele/mesa", 7, "sete-velho");
        avisar(&avisador, &saiu, "seele/mesa", 8, "oito");
        avisar(&avisador, &saiu, "seele/mesa", 7, "sete-novo");
        passar(Duration::from_secs(1)).await;
        let depois: Vec<String> = cargas(&saiu).into_iter().skip(4).collect();
        assert_eq!(
            depois,
            ["sete-novo", "oito"],
            "o excedente não ficou por canal"
        );
    }

    /// A taxa é **por MOD**: um MOD que avisa sem parar não segura os avisos do
    /// vizinho.
    #[tokio::test(start_paused = true)]
    async fn um_mod_nao_gasta_a_taxa_do_outro() {
        let (avisador, saiu) = avisador();
        for n in 1..=10 {
            avisar(&avisador, &saiu, "seele/mesa", 1, &n.to_string());
        }
        avisar(&avisador, &saiu, "seele/perfis", 1, "do-perfis");
        assert!(
            cargas(&saiu).contains(&"do-perfis".to_owned()),
            "o aviso de um MOD esperou a taxa do outro"
        );
    }
}
