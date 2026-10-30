//! Regression: ICE nomination may arrive after the first connectivity check.
//! Include from ponta.rs with #[cfg(test)] mod dtls_prazo_testes;
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use super::*;

fn atributo(pacote: &mut Vec<u8>, tipo: u16, valor: &[u8]) {
    pacote.extend_from_slice(&tipo.to_be_bytes());
    pacote.extend_from_slice(&(valor.len() as u16).to_be_bytes());
    pacote.extend_from_slice(valor);
    while pacote.len() % 4 != 0 {
        pacote.push(0);
    }
}

/// RFC 5389 Binding request with correct MESSAGE-INTEGRITY; nomination optional.
fn binding(u: &str, nomeia: bool, transacao: u8) -> Vec<u8> {
    let mut p = vec![0; 20];
    p[..2].copy_from_slice(&1_u16.to_be_bytes());
    p[4..8].copy_from_slice(&0x2112_A442_u32.to_be_bytes());
    p[8..20].fill(transacao);
    atributo(&mut p, 0x0006, format!("{u}:{u}").as_bytes()); // USERNAME
    atributo(&mut p, 0x0024, &2_130_706_431_u32.to_be_bytes()); // PRIORITY
    atributo(&mut p, 0x802a, &123_456_789_u64.to_be_bytes()); // ICE-CONTROLLING
    if nomeia {
        atributo(&mut p, 0x0025, &[]);
    } // USE-CANDIDATE
      // Length includes MI, HMAC input ends immediately before its attribute.
    let tamanho = (p.len() - 20 + 24) as u16;
    p[2..4].copy_from_slice(&tamanho.to_be_bytes());
    let mac = crate::cripto::hmac_sha1(u.as_bytes(), &[&p]);
    atributo(&mut p, 0x0008, &mac);
    let lido = str0m::ice::StunMessage::parse(&p).expect("STUN legivel");
    assert!(lido.is_binding_request());
    assert!(
        lido.verify(u.as_bytes(), |chave, partes| crate::cripto::hmac_sha1(
            chave, partes
        ))
    );
    assert_eq!(binding_com_u(&p).as_deref(), Some(u));
    p
}

async fn motor() -> Motor {
    let cru = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    cru.set_nonblocking(true).unwrap();
    let local = cru.local_addr().unwrap();
    let saida = Arc::new(cru.try_clone().unwrap());
    let socket = Arc::new(UdpSocket::from_std(cru).unwrap());
    let (cmd, _comandos) = mpsc::unbounded_channel();
    let (chegadas, _recebidas) = mpsc::channel(64);
    let (enc, _encontros) = mpsc::channel(64);
    Motor {
        socket,
        saida,
        local,
        anfitriao: Some(ConfigDoAnfitriao::novo(CertificadoDtls::efemero().unwrap())),
        pares: HashMap::new(),
        credenciais: HashMap::new(),
        donos: HashMap::new(),
        lapides: HashMap::new(),
        ordem_das_lapides: VecDeque::new(),
        falhas_por_ip: HashMap::new(),
        criacoes_por_ip: HashMap::new(),
        cmd: cmd.downgrade(),
        chegadas,
        enc,
        vivas: Arc::new(AtomicUsize::new(0)),
        contadores: Arc::new(ContadoresDaPontaVivos::default()),
    }
}

async fn caso(atraso: Duration, pelo_chamado: bool) {
    let mut m = motor().await;
    let remoto: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let u = crate::sortear_credencial();
    let t0 = Instant::now();
    if pelo_chamado {
        m.chamado(t0, u.clone(), vec![remoto]);
    }
    m.chegou(t0, remoto, &binding(&u, false, 1));
    let id = m.credenciais[&u];
    assert!(m.pares[&id].espera_do_dtls_desde.is_none());
    assert!(!m.pares[&id].dtls_visto);
    m.tique(t0 + atraso);
    assert!(
        m.pares.contains_key(&id),
        "ICE ainda pode nomear dentro do prazo geral"
    );
    assert_eq!(m.contadores.pendentes_sem_dtls.load(Ordering::Relaxed), 0);
    m.chegou(t0 + atraso, remoto, &binding(&u, true, 2));
    assert!(m.pares.contains_key(&id));
    assert_eq!(m.contadores.bindings_em_lapide.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn nomeacao_antes_de_2s_preserva_par_controle() {
    caso(Duration::from_millis(1500), true).await;
}

#[tokio::test]
async fn nomeacao_depois_de_2s_preserva_par_do_chamado() {
    caso(Duration::from_millis(2500), true).await;
}

#[tokio::test]
async fn nomeacao_depois_de_2s_preserva_par_direto() {
    caso(Duration::from_millis(2500), false).await;
}

#[tokio::test]
async fn checagens_sem_nomeacao_nao_prolongam_prazo_geral() {
    let mut m = motor().await;
    let remoto = "127.0.0.1:54321".parse().unwrap();
    let u = crate::sortear_credencial();
    let t0 = Instant::now();
    m.chamado(t0, u.clone(), vec![remoto]);
    m.chegou(t0, remoto, &binding(&u, false, 1));
    let id = m.credenciais[&u];
    for segundo in 1..10 {
        let agora = t0 + Duration::from_secs(segundo);
        m.chegou(agora, remoto, &binding(&u, false, segundo as u8 + 1));
        m.tique(agora);
        assert!(m.pares.contains_key(&id));
    }
    m.tique(t0 + crate::APERTO + Duration::from_millis(1));
    assert!(!m.pares.contains_key(&id));
    assert!(m.em_lapide(t0 + crate::APERTO, &u));
}

#[tokio::test]
async fn nomeacao_inicia_prazo_dtls_sem_renovar_em_eventos_repetidos() {
    let mut m = motor().await;
    let remoto = "127.0.0.1:54321".parse().unwrap();
    let u = crate::sortear_credencial();
    let t0 = Instant::now();
    m.chamado(t0, u.clone(), vec![remoto]);
    let id = m.credenciais[&u];
    let nomeado = t0 + Duration::from_secs(3);
    // O evento é emitido pelo str0m depois de validar a nomeação. O teste de
    // integração portoes cobre a recepção real de um Binding com USE-CANDIDATE.
    m.pares.get_mut(&id).unwrap().ultimo_rx = nomeado;
    m.evento(
        id,
        Event::IceConnectionStateChange(str0m::IceConnectionState::Connected),
    );
    assert_eq!(m.pares[&id].espera_do_dtls_desde, Some(nomeado));
    m.pares.get_mut(&id).unwrap().ultimo_rx = nomeado + Duration::from_secs(1);
    m.evento(
        id,
        Event::IceConnectionStateChange(str0m::IceConnectionState::Completed),
    );
    assert_eq!(m.pares[&id].espera_do_dtls_desde, Some(nomeado));
    m.tique(nomeado + ESPERA_PELO_DTLS + Duration::from_millis(1));
    assert!(!m.pares.contains_key(&id));
    assert_eq!(m.contadores.pendentes_sem_dtls.load(Ordering::Relaxed), 1);
}
