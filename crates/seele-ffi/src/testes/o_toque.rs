//! O toque da sala pessoal na saída escolhida, e na padrão quando a
//! escolhida sumiu: um fone desligado não pode custar uma ligação. Provado
//! sem abrir aparelho nenhum, pelo `tocar_com` que recebe quem abre.

use super::{tocar_com, ConnectionError};
use seele_core::DeviceError;

#[test]
fn a_saida_escolhida_toca_quando_abre() {
    let mut pedidas = Vec::new();
    let tocado = tocar_com(Some("fone".to_owned()), |saida| {
        pedidas.push(saida.clone());
        Ok::<_, DeviceError>(saida)
    });
    assert_eq!(tocado, Ok(Some("fone".to_owned())));
    assert_eq!(pedidas, vec![Some("fone".to_owned())]);
}

#[test]
fn a_saida_escolhida_que_sumiu_cai_na_padrao() {
    let mut pedidas = Vec::new();
    let tocado = tocar_com(Some("fone".to_owned()), |saida| {
        pedidas.push(saida.clone());
        match saida {
            Some(id) => Err(DeviceError::PlaybackDeviceGone { id }),
            None => Ok("padrão"),
        }
    });
    assert_eq!(
        tocado,
        Ok("padrão"),
        "um fone desligado calou o toque: a ligação seria perdida sem som"
    );
    assert_eq!(pedidas, vec![Some("fone".to_owned()), None]);
}

#[test]
fn sem_saida_nenhuma_o_toque_diz_que_nao_abriu() {
    let tocado = tocar_com(None, |_| Err::<(), _>(DeviceError::NoOutputDevice));
    assert_eq!(tocado, Err(ConnectionError::NoAudioDevice));
    let tocado = tocar_com(Some("fone".to_owned()), |saida| {
        Err::<(), _>(match saida {
            Some(id) => DeviceError::PlaybackDeviceGone { id },
            None => DeviceError::NoOutputDevice,
        })
    });
    assert_eq!(tocado, Err(ConnectionError::NoAudioDevice));
}
