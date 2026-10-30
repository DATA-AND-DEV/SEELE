//! Convites IPv6 compartilhados: equivalência de URL e preservação da âncora.
use seele_proto::uri::{analisar, analisar_no_navegador, Bilhete, Convite, ORIGEM_DO_NAVEGADOR};

fn convite() -> Convite {
    Convite::novo("192.168.0.32:8383")
        .com_alternativos(["198.51.100.7:13852", "[2001:db8::1]:8383"])
        .com_bilhete(Bilhete::novo("rendzvous.seele.sh:8384", "198.51.100.7:13852").unwrap())
        .com_impressao_digital("ab".repeat(32))
        .com_token("7K4MNPQRSTVWXYZ23456")
        .com_versao("2.0")
}

#[test]
fn ipv6_cru_e_codificado_preservam_o_convite_inteiro() {
    let original = convite();
    let cru = format!(
        "https://seele.sh/#{}",
        original.to_string().trim_start_matches("seele://")
    );
    for texto in [
        cru.clone(),
        cru.replace('[', "%5B").replace(']', "%5D"),
        cru.replace('[', "%5b").replace(']', "%5d"),
    ] {
        assert_eq!(
            analisar_no_navegador(&texto, ORIGEM_DO_NAVEGADOR)
                .unwrap()
                .convite,
            original
        );
        let desktop = analisar(&texto).unwrap();
        assert_eq!(desktop.alternativos, original.alternativos);
        assert_eq!(desktop.impressao_digital, original.impressao_digital);
        assert_eq!(desktop.token, original.token);
    }
}

#[test]
fn link_gerado_nao_tem_colchetes_crus_e_volta_sem_perdas() {
    for alvo in ["192.168.0.32:8383", "[2001:db8::2]:8383"] {
        let mut original = convite();
        original.alvo = alvo.to_owned();
        let link = original.link_do_navegador().unwrap();
        assert!(!link.contains(['[', ']']));
        assert!(link.contains("%5B2001:db8::1%5D"));
        assert_eq!(
            analisar_no_navegador(&link, ORIGEM_DO_NAVEGADOR)
                .unwrap()
                .convite,
            original
        );
        assert_eq!(link.split('#').next(), Some("https://seele.sh/"));
    }
}

#[test]
fn codificacao_nao_recupera_pin_ausente_nem_injeta_campos() {
    let link = convite().link_do_navegador().unwrap();
    for adulterado in [
        link.split("&fp=").next().unwrap().to_owned(),
        link.replace("%5B", "%255B"),
        link.replace("%5D", "%5"),
        link.replace("%5D", "%5D%26fp%3D"),
        link.replace("192.168.0.32", "192.168.0.32%3Ffp%3D"),
    ] {
        assert!(analisar_no_navegador(&adulterado, ORIGEM_DO_NAVEGADOR).is_err());
    }
    assert!(analisar_no_navegador(
        &link.replace("https://seele.sh/", "https://seele.sh.evil.invalid/"),
        ORIGEM_DO_NAVEGADOR
    )
    .is_err());
}
