//! A lista de variantes de um enum do fio, perguntada ao próprio `serde`.
//!
//! O postcard escreve uma variante pelo **ordinal**, a posição dela na lista.
//! Saber quantas variantes uma build conhece, e em que ordem, é o que separa
//! uma mensagem desconhecida de uma malformada (`control::decode_da_lista`) e
//! o que os guardas da linha 1 prendem (`tests/fio_1_0.rs`, ADR 0058).
//!
//! # Por que perguntar ao `serde`, e não decodificar cargas de zeros
//!
//! O `derive` do `serde` entrega a lista de nomes das variantes ao
//! `Deserializer` em `deserialize_enum`, **antes** de qualquer campo. O
//! [`Perguntador`] abaixo é um `Deserializer` que só sabe atender a essa
//! chamada: ele anota a lista e desiste. Nenhum campo é lido, então nenhum tipo
//! de campo muda a resposta — uma variante cujo corpo não aceita zeros (um
//! `NonZeroU32`, um `char`) é contada como qualquer outra. A história de por que
//! isso importa está em `control.rs`, no guarda
//! `uma_variante_que_nao_aceita_zeros_ainda_e_contada`.
//!
//! Morava nos testes de `control.rs`, e saiu de lá porque passou a ser usada
//! fora deles: a leitura de uma mensagem desconhecida precisa do número, e o
//! teste de integração dos bytes-ouro precisa dos nomes.

use std::cell::Cell;
use std::fmt;

/// Os nomes das variantes do enum `T`, na ordem dos ordinais.
///
/// `None` quando `T` não é um enum: o `Deserializer` só foi perguntado por
/// outra forma, e não há lista para dar.
#[must_use]
pub fn variantes<T>() -> Option<&'static [&'static str]>
where
    T: for<'a> serde::Deserialize<'a>,
{
    let lista = Cell::new(None);
    let _ = T::deserialize(Perguntador(&lista));
    lista.get()
}

/// Como o [`Perguntador`] desiste: um erro que só carrega o porquê.
#[derive(Debug)]
struct Desistencia(String);

impl fmt::Display for Desistencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Desistencia {}

impl serde::de::Error for Desistencia {
    fn custom<M: fmt::Display>(msg: M) -> Self {
        Self(msg.to_string())
    }
}

/// Um `Deserializer` que atende só `deserialize_enum`, anota a lista e erra.
struct Perguntador<'a>(&'a Cell<Option<&'static [&'static str]>>);

impl<'de> serde::Deserializer<'de> for Perguntador<'_> {
    type Error = Desistencia;

    fn deserialize_enum<V>(
        self,
        _nome: &'static str,
        variantes: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.0.set(Some(variantes));
        Err(serde::de::Error::custom(
            "a pergunta era quais variantes existem, e ela já foi respondida",
        ))
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(serde::de::Error::custom(
            "este Deserializer só sabe responder por um enum",
        ))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str
        string bytes byte_buf option unit unit_struct newtype_struct seq
        tuple tuple_struct map struct identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_nomes_saem_na_ordem_dos_ordinais() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        enum Lista {
            Um,
            Dois(u8),
            Tres { campo: char },
        }
        assert_eq!(variantes::<Lista>(), Some(&["Um", "Dois", "Tres"][..]));
    }

    #[test]
    fn o_que_nao_e_enum_nao_tem_lista() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct NaoEnum {
            campo: u8,
        }
        assert_eq!(variantes::<NaoEnum>(), None);
        assert_eq!(variantes::<u32>(), None);
    }
}
