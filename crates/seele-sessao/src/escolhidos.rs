//! **Os arquivos que uma pessoa escolheu para um MOD** (rascunho da API 6,
//! §7): as regras do seletor e de quem segura o quê, para o desktop e o
//! navegador (Tarefa 2.6 da F4). Saíram do `main.rs` do app.
//!
//! O guarda é genérico no que ele segura: o desktop guarda os bytes nele, e o
//! navegador guarda só o tamanho, com os bytes na página — a memória de um
//! `.wasm` cresce e não encolhe, e quatro arquivos de dez megabytes ficariam lá
//! pela vida da página.

use crate::mods::TipoLido;

/// **Quantos arquivos escolhidos uma sessão segura ao mesmo tempo.**
///
/// Quatro. Um MOD que abre o seletor num laço não pode encher a memória de quem
/// está numa conversa, e quatro é mais do que qualquer fluxo real precisa: um
/// retrato, uma faixa, uma cena e uma folga.
pub const ARQUIVOS_DE_PE: usize = 4;

/// **O maior arquivo que uma pessoa pode escolher para um MOD.**
///
/// Dez megabytes, que é o teto que os MODs oficiais já praticavam do lado do
/// servidor. Ele não atravessa inteiro: sai em pedaços que o MOD pede.
pub const TETO_DO_ESCOLHIDO: usize = 10 * 1024 * 1024;

/// Quanto de um arquivo escolhido sai por pedido.
///
/// **Múltiplo de três**, e é o que importa aqui. O base64 codifica três bytes
/// em quatro caracteres; uma janela que não fecha um grupo de três sai com
/// `=` no meio, e dois pedaços consecutivos concatenados deixam de ser o
/// arquivo. `65536` não é múltiplo de três — `65535` é —, e a diferença entre
/// os dois é uma imagem que chega corrompida sem nada acusar.
pub const PEDACO_DO_ESCOLHIDO: usize = 65535;

/// As extensões que o seletor oferece, entre as que o MOD pedir: o que os
/// bytes podem provar ser (as imagens e os sons do `seele_proto`).
pub const EXTENSOES_CONHECIDAS: [&str; 10] = [
    "png", "jpg", "jpeg", "webp", "gif", "wav", "mp3", "ogg", "flac", "m4a",
];

/// O título do seletor: a finalidade que o MOD deu, sem controle e com no
/// máximo 120 caracteres, ou a frase do produto.
#[must_use]
pub fn titulo_do_seletor(finalidade: Option<&str>) -> String {
    let limpo: String = finalidade
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    let limpo = limpo.trim();
    if limpo.is_empty() {
        "Escolha um arquivo para este MOD".to_owned()
    } else {
        limpo.to_owned()
    }
}

/// As extensões que o seletor filtra: as que o MOD pediu, na ordem dele, que
/// o produto conhece (sem caixa), na grafia do produto.
#[must_use]
pub fn extensoes_do_seletor(pedidas: Option<&[String]>) -> Vec<&'static str> {
    pedidas
        .unwrap_or_default()
        .iter()
        .filter_map(|pedida| {
            EXTENSOES_CONHECIDAS
                .iter()
                .find(|conhecida| conhecida.eq_ignore_ascii_case(pedida))
                .copied()
        })
        .collect()
}

/// O teto deste pedido de escolha: o do produto, que vale sempre, apertado
/// pelo que o MOD declarou. Um MOD não amplia o que esta máquina aceita ler, e
/// zero é «não declarou».
#[must_use]
pub fn teto_do_escolhido(limite_de_bytes: Option<usize>) -> usize {
    limite_de_bytes
        .filter(|n| *n > 0)
        .map_or(TETO_DO_ESCOLHIDO, |n| n.min(TETO_DO_ESCOLHIDO))
}

/// **O tipo de um arquivo que uma pessoa escolheu**, do que os bytes são e
/// nunca da extensão. Um formato que o produto reconhece e não aceita — o HEIC
/// das Fotos — é recusado pelo nome dele, `tipo-recusado:heic` (rascunho da API
/// 6, §7.1).
///
/// # Errors
///
/// `tipo-recusado:<formato>` e `formato-desconhecido`.
pub fn tipo_do_escolhido(bytes: &[u8]) -> Result<TipoLido, String> {
    crate::mods::ler_tipo(bytes).ok_or_else(|| {
        crate::mods::reconhecido_e_nao_aceito(bytes).map_or_else(
            || "formato-desconhecido".to_owned(),
            |formato| format!("tipo-recusado:{formato}"),
        )
    })
}

/// O papel provado dos bytes contra os que o MOD pediu. Nenhum pedido (ou a
/// lista vazia) aceita os dois; um papel fora da lista é
/// `papel-nao-serve:<papel>`, com o nome, para a região dizê-lo.
///
/// # Errors
///
/// `papel-nao-serve:<papel>`.
pub fn o_papel_serve(papeis: Option<&[String]>, papel: &str) -> Result<(), String> {
    match papeis.filter(|p| !p.is_empty()) {
        Some(pedidos) if !pedidos.iter().any(|p| p == papel) => {
            Err(format!("papel-nao-serve:{papel}"))
        }
        _ => Ok(()),
    }
}

/// Um arquivo que uma pessoa escolheu, guardado pelo produto.
#[derive(Debug)]
pub struct Escolhido<B> {
    /// De quem é. Um MOD não lê o arquivo que outro pediu.
    pub de_quem: String,
    /// Em que sessão foi escolhido: a saída leva todos os dela.
    pub geracao: u64,
    /// O que os bytes provaram ser, para o diagnóstico saber o que está de pé
    /// sem abrir os bytes.
    pub tipo: &'static str,
    /// Quantos bytes ele tem.
    pub tamanho: usize,
    /// O que quem guarda segura: os bytes, no desktop; nada, no navegador.
    pub conteudo: B,
}

/// Os arquivos escolhidos desta janela, por identificador.
#[derive(Debug)]
pub struct Escolhidos<B> {
    /// O próximo identificador. Monotônico: um número nunca se repete.
    proximo: u64,
    guardados: std::collections::BTreeMap<u64, Escolhido<B>>,
}

impl<B> Default for Escolhidos<B> {
    fn default() -> Self {
        Self {
            proximo: 0,
            guardados: std::collections::BTreeMap::new(),
        }
    }
}

impl<B> Escolhidos<B> {
    /// Guarda um, e devolve o número dele.
    ///
    /// # Errors
    ///
    /// `arquivos-demais-de-pe`, quando a sessão já segura
    /// [`ARQUIVOS_DE_PE`].
    pub fn guardar(&mut self, arquivo: Escolhido<B>) -> Result<u64, String> {
        if self.guardados.len() >= ARQUIVOS_DE_PE {
            return Err("arquivos-demais-de-pe".to_owned());
        }
        self.proximo = self.proximo.saturating_add(1);
        let numero = self.proximo;
        self.guardados.insert(numero, arquivo);
        Ok(numero)
    }

    /// Solta os de uma sessão que acabou, e diz quais.
    pub fn soltar_geracao(&mut self, geracao: u64) -> Vec<u64> {
        let soltos: Vec<u64> = self
            .guardados
            .iter()
            .filter(|(_, a)| a.geracao == geracao)
            .map(|(numero, _)| *numero)
            .collect();
        for numero in &soltos {
            self.guardados.remove(numero);
        }
        soltos
    }

    /// O de `arquivo`, se é de `id` nesta `geracao`.
    ///
    /// # Errors
    ///
    /// `arquivo-nao-esta-de-pe` e `arquivo-de-outro-mod`.
    pub fn de(&self, geracao: u64, id: &str, arquivo: u64) -> Result<&Escolhido<B>, String> {
        let guardado = self
            .guardados
            .get(&arquivo)
            .ok_or_else(|| "arquivo-nao-esta-de-pe".to_owned())?;
        // **De quem pediu, e não de quem souber o número.** Sem esta linha, um
        // MOD que adivinhasse um número leria o arquivo que outro pediu — e o
        // número é pequeno e sequencial.
        if guardado.de_quem != id || guardado.geracao != geracao {
            return Err("arquivo-de-outro-mod".to_owned());
        }
        Ok(guardado)
    }

    /// Os bytes de um pedaço: de `inicio`, até [`PEDACO_DO_ESCOLHIDO`] ou o
    /// fim. Vazio depois do fim.
    ///
    /// # Errors
    ///
    /// Os de [`Self::de`].
    pub fn pedaco(
        &self,
        geracao: u64,
        id: &str,
        arquivo: u64,
        inicio: usize,
    ) -> Result<std::ops::Range<usize>, String> {
        let guardado = self.de(geracao, id, arquivo)?;
        let fim = inicio
            .saturating_add(PEDACO_DO_ESCOLHIDO)
            .min(guardado.tamanho);
        Ok(inicio.min(fim)..fim)
    }

    /// Tira o de `arquivo` para mandá-lo: ele é consumido antes do envio, dê
    /// o envio certo ou não.
    ///
    /// # Errors
    ///
    /// Os de [`Self::de`].
    pub fn tirar(&mut self, geracao: u64, id: &str, arquivo: u64) -> Result<Escolhido<B>, String> {
        self.de(geracao, id, arquivo)?;
        self.guardados
            .remove(&arquivo)
            .ok_or_else(|| "arquivo-nao-esta-de-pe".to_owned())
    }

    /// Solta o de `arquivo`, se é de `id` nesta `geracao`; o resto não faz
    /// nada. Diz se soltou.
    pub fn soltar(&mut self, geracao: u64, id: &str, arquivo: u64) -> bool {
        if self.de(geracao, id, arquivo).is_ok() {
            self.guardados.remove(&arquivo);
            return true;
        }
        false
    }

    /// Quantos estão de pé.
    #[must_use]
    pub fn len(&self) -> usize {
        self.guardados.len()
    }

    /// Se nenhum está de pé.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.guardados.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escolhido(de_quem: &str, geracao: u64, tamanho: usize) -> Escolhido<()> {
        Escolhido {
            de_quem: de_quem.to_owned(),
            geracao,
            tipo: "image/png",
            tamanho,
            conteudo: (),
        }
    }

    #[test]
    fn quatro_de_pe_de_quem_pediu_e_os_pedacos_fecham_grupos_de_tres() {
        let mut guardados = Escolhidos::default();
        for _ in 0..ARQUIVOS_DE_PE {
            guardados.guardar(escolhido("a/b", 1, 200_000)).unwrap();
        }
        assert_eq!(
            guardados.guardar(escolhido("a/b", 1, 1)),
            Err("arquivos-demais-de-pe".to_owned())
        );
        assert_eq!(guardados.pedaco(1, "a/b", 1, 0), Ok(0..65535));
        assert_eq!(guardados.pedaco(1, "a/b", 1, 196_605), Ok(196_605..200_000));
        assert_eq!(guardados.pedaco(1, "a/b", 1, 300_000), Ok(200_000..200_000));
        assert_eq!(
            guardados.pedaco(1, "c/d", 1, 0),
            Err("arquivo-de-outro-mod".to_owned())
        );
        assert_eq!(
            guardados.pedaco(2, "a/b", 1, 0),
            Err("arquivo-de-outro-mod".to_owned())
        );
        assert_eq!(
            guardados.pedaco(1, "a/b", 9, 0),
            Err("arquivo-nao-esta-de-pe".to_owned())
        );
        assert!(!guardados.soltar(1, "c/d", 2));
        assert!(guardados.soltar(1, "a/b", 2));
        assert!(guardados.tirar(1, "a/b", 3).is_ok());
        assert_eq!(
            guardados.tirar(1, "a/b", 3).err(),
            Some("arquivo-nao-esta-de-pe".to_owned())
        );
        // A saída da sessão leva os que sobraram dela (o 1 e o 4).
        let mut soltos = guardados.soltar_geracao(1);
        soltos.sort_unstable();
        assert_eq!(soltos, vec![1, 4]);
        assert!(guardados.is_empty());
    }
}
