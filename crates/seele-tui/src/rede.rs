//! `plug --rede` — o instrumento que transforma relato de campo em dado.
//!
//! Ligue em `main.rs` com **uma linha**, antes de o terminal alternativo abrir:
//! `if rede::pedido(&argumentos) { rede::rodar_e_sair(&argumentos) }`, onde
//! `argumentos` é `std::env::args().skip(1).collect::<Vec<String>>()` — o nome
//! do programa não entra.
//!
//! # O que ele afirma, e o que ele recusa afirmar
//!
//! O ADR 0022 existe porque o produto já mentiu sobre alcance uma vez: um
//! endereço de túnel foi anunciado como "alcança de qualquer lugar", e quem
//! hospedava passou uma tarde tentando. Este comando é o contrário disso — cada
//! linha da saída é um fato medido ou a palavra `desconhecido`, e nunca uma
//! dedução travestida de medida.
//!
//! **O tipo de NAT.** Distinguir cone de simétrico exige comparar o mapeamento
//! do **mesmo socket local** visto de **dois destinos diferentes**. O
//! `SEELE-ENC/1` não dá isso com um ponto de encontro só: `ONDE` responde pelo
//! socket que recebeu e `LEVE` reflete a partir do mesmo socket, então a origem
//! de todo `AQUI` é `IP-do-ponto:8384`, invariavelmente. Por isso este comando
//! aceita **N** pontos de encontro (`--ponto`, repetível) e só classifica com
//! dois ou mais. Com um, a saída diz `desconhecido`, e a palavra é honesta.
//!
//! Dois pontos de encontro **em máquinas diferentes**, e a diferença importa:
//! dois pedidos ao mesmo serviço sempre concordam, porque o mapeamento é o
//! mesmo — e a saída afirmaria `cone`, o único veredicto que manda continuar
//! tentando, a partir de um ponto de vista só. Alvo repetido é **recusado**, com
//! erro e não em silêncio. O que a recusa não pega é dois nomes DNS diferentes
//! da mesma máquina, e por isso esta frase; e mesmo com serviços diferentes, um
//! NAT de mapeamento dependente do **endereço** aloca por IP de destino e ainda
//! pareceria cone. `docs/ponto-de-encontro.md` são dez linhas de comando para
//! quem quiser subir o segundo.
//!
//! O que uma máquina só afirma com certeza, e ainda vale: **se o endereço
//! observado é um dos endereços desta máquina, não há NAT no caminho.** É o
//! degrau 1 do ADR 0022 medido em vez de deduzido, e não precisa de segundo
//! ponto de vista nenhum.
//!
//! **A entrada de fora.** `LEVE <meu próprio endereço global:porta alta>` faz o
//! ponto de encontro mandar um datagrama **não solicitado** a um socket que
//! nunca falou com ele — aqui, um segundo socket, o *ouvinte*, que em toda a
//! execução não manda um byte para lugar nenhum. Se aqueles 96 bytes chegam,
//! entrada de fora funciona de verdade: é o único teste do projeto que
//! transforma o "chance, e não certeza" de `Degrau::alcanca_de_fora` em fato
//! medido, e pega **de fora** o sucesso mentiroso do CGNAT.
//!
//! O limite é honesto e está na saída, não só neste comentário: prova que 96
//! bytes daquela origem chegaram àquela porta, **não** que o aperto de mão QUIC
//! sobe. E o endereço para onde o `LEVE` aponta é o IP global observado com a
//! porta do ouvinte. Num endereço próprio — que é onde a pergunta "o firewall
//! do roteador deixa entrar?" importa — essa porta é a porta pública e a medida
//! é exata; atrás de um NAT que reescreve porta, um "chegou" continua sendo
//! prova e um "não chegou" não é. A saída diz qual dos dois casos foi, em vez
//! de dar o mesmo peso aos dois.
//!
//! **O furo.** Furo de verdade precisa de duas máquinas em redes diferentes, e
//! nenhuma execução solitária pode dizer que ele falhou. Fora do modo par a
//! linha diz `não testado`, **nunca** `FALHOU`. O modo par é `plug --rede
//! --esperar` de um lado, que imprime um bilhete, e `plug --rede <bilhete>` do
//! outro: é o degrau 4 inteiro, sem servidor atrás.
//!
//! # Por que aqui dentro, e não num binário novo
//!
//! O peso que o ADR 0022 cobra é a árvore de dependências do daemon, e este
//! diagnóstico não acrescenta nenhuma: `seele-tui` já vê `seele-core` **e**
//! `seele-server`, pela exceção nomeada em `xtask/src/check_deps.rs`. Nada entra
//! em `seele-server` por causa disto e nada roda sem ser pedido. Um `[[bin]]`
//! novo custaria um link inteiro mais três linhas em `empacotar/` mais um
//! `externalBin` no Tauri; um exemplo não serviria, porque quem tem a rede
//! quebrada não tem `cargo`.

use std::fmt::Write as _;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use seele_core::uri::Bilhete;
use seele_server::alcance::encontro::{PONTO_PADRAO, VARIAVEL};
use seele_server::alcance::interfaces::{self, Achado, Origem};
use seele_server::encontro::{self as protocolo, Marca};

/// Quanto se espera por cada resposta de um ponto de encontro.
///
/// O mesmo segundo de `alcance::encontro::PRAZO`, e pela mesma conta: é uma ida
/// e volta a um servidor na internet, e ou volta rápido ou está bloqueado.
const PRAZO: Duration = Duration::from_secs(1);

/// De quanto em quanto tempo a pergunta é repetida enquanto o prazo corre.
///
/// Um datagrama se perde, e perder um `ONDE` faria a saída dizer "não respondeu"
/// sobre um ponto de encontro que está de pé.
const REPETICAO: Duration = Duration::from_millis(300);

/// Quanto o modo par espera pelo outro lado.
///
/// Generoso de propósito: do outro lado tem uma pessoa lendo um bilhete numa
/// conversa e colando num terminal, e o prazo tem de caber isso.
const PRAZO_DO_PAR: Duration = Duration::from_secs(45);

/// A marca do `LEVE` do modo par.
///
/// O lado que espera reconhece por ela o `AQUI` que o ponto de encontro
/// repassou — é a única forma de separar o aviso do par do eco da própria
/// pergunta, que volta com outra marca.
const MARCA_DO_PAR: &str = "par";

/// As três batidas do furo, e por que são três.
///
/// Com duas — furo e eco — o lado que **eco**a declara sucesso sem confirmação
/// nenhuma: ele sabe que recebeu, e não sabe se o que ele mandou de volta
/// chegou. Um eco perdido fazia um lado imprimir «abriu» e o outro «FALHOU»
/// sobre a mesma tentativa, e a saída que existe para não mentir mentia para
/// exatamente uma das duas pessoas.
///
/// Com três, cada lado só afirma sobre o que **viu voltar**: quem espera vê a
/// `volta` (e ela só existe se a `ida` chegou), e quem bate vê o `pronto` (e ele
/// só existe se a `volta` chegou). Nenhum dos dois declara sucesso sobre um
/// sentido que não mediu.
const MARCA_IDA: &str = "ida";
/// Ver [`MARCA_IDA`].
const MARCA_VOLTA: &str = "volta";
/// Ver [`MARCA_IDA`].
const MARCA_PRONTO: &str = "pronto";

/// A largura da coluna dos rótulos, para a saída ficar em duas colunas.
const ROTULO: usize = 19;

/// A primeira linha da saída, e a régua que vai debaixo dela.
const TITULO: &str = "REDE — o que esta máquina alcança";

/// O que dá para dizer sobre o NAT desta máquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nat {
    /// O endereço observado é desta máquina: não há NAT no caminho.
    Nenhum,
    /// Dois pontos de encontro viram o mesmo mapeamento.
    Cone,
    /// Dois pontos viram mapeamentos diferentes. É o caso que o ADR 0022 deixou
    /// sem saída: o endereço que um ponto vê não é por onde o outro lado
    /// chegaria, e a resposta a isso seria retransmissão.
    Simetrico,
    /// Um ponto de encontro só. **Não dá para saber**, e dizer que dá seria pior
    /// que calar.
    Desconhecido,
}

/// A classificação inteira, e ela é curta porque o protocolo é curto.
///
/// `vistos` traz **um mapeamento por ponto de encontro**, todos da mesma
/// família: comparar dois mapeamentos do mesmo ponto não diz nada, e comparar um
/// IPv4 com um IPv6 compara dois caminhos.
fn classificar_nat(vistos: &[SocketAddr], meus: &[IpAddr]) -> Nat {
    let Some(primeiro) = vistos.first() else {
        return Nat::Desconhecido;
    };
    if meus.contains(&primeiro.ip()) {
        return Nat::Nenhum;
    }
    match vistos.get(1) {
        None => Nat::Desconhecido,
        Some(segundo) if segundo == primeiro => Nat::Cone,
        Some(_) => Nat::Simetrico,
    }
}

/// Se o furo foi medido, e o que ele deu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Furo {
    /// Fora do modo par. **Nunca** vira `FALHOU`: furo de verdade precisa de
    /// duas máquinas em redes diferentes, e uma execução solitária não tem como
    /// saber.
    NaoTestado,
    /// Os 96 bytes atravessaram nos dois sentidos, e **os dois lados
    /// confirmaram**.
    Aberto,
    /// Modo par, avisos mandados, e o outro lado não chegou dentro do prazo.
    ///
    /// É a **única** variante que imprime `FALHOU`, e o motivo é o que separa
    /// esta ferramenta do roteiro que ela veio substituir: `FALHOU` é uma causa
    /// medida, e as três variantes abaixo são fracassos que nunca chegaram a
    /// mandar um pacote. Dar a todos a mesma cara seria reconstruir aqui dentro
    /// o defeito que se estava consertando.
    NaoAbriu,
    /// O bilhete não traz um endereço para avisar. Nada foi mandado.
    BilheteSemEndereco,
    /// O ponto de encontro do bilhete não resolve. Nada foi mandado.
    PontoDoBilheteNaoResolve,
    /// `--esperar`, e nenhum ponto de encontro respondeu: não há bilhete a dar,
    /// e portanto não há como o outro lado sequer tentar.
    SemBilheteAAnunciar,
}

/// O que esta execução vai medir.
#[derive(Debug, Clone)]
struct Plano {
    /// Os pontos de encontro, na ordem em que serão perguntados.
    pontos: Vec<String>,
    /// Modo par, lado de quem espera.
    esperar: bool,
    /// Modo par, lado de quem tem o bilhete.
    bilhete: Option<Bilhete>,
    /// Quanto se espera por cada resposta.
    prazo: Duration,
    /// Quanto se espera pelo outro lado, no modo par.
    prazo_do_par: Duration,
    /// Se o laço local de QUIC é medido. O teste que não é sobre ele pula.
    quic: bool,
}

/// O que um ponto de encontro respondeu, por endereço dele.
#[derive(Debug, Clone, Copy)]
struct Resposta {
    /// O endereço do ponto de encontro a que se perguntou.
    alvo: SocketAddr,
    /// De onde ele disse que esta máquina fala. `None` é silêncio.
    visto: Option<SocketAddr>,
}

/// Tudo o que um ponto de encontro rendeu.
#[derive(Debug, Clone)]
struct Sondagem {
    /// Como ele foi escrito na linha de comando ou no ambiente.
    ponto: String,
    /// Uma por família que o nome resolveu. Vazio quer dizer que não resolveu.
    respostas: Vec<Resposta>,
}

/// O que foi medido, antes de virar texto.
#[derive(Debug, Clone)]
struct Fatos {
    achados: Vec<Achado>,
    meus: Vec<IpAddr>,
    sondagens: Vec<Sondagem>,
    nat: Nat,
    /// Uma por família sondada: o nome dela, se os 96 bytes chegaram, e se um
    /// "não chegaram" é conclusivo.
    ///
    /// O terceiro campo é a diferença entre uma medida e um palpite. O `LEVE`
    /// aponta para o IP global observado com a **porta do ouvinte**, e essa
    /// porta só é a porta pública dele se ninguém reescreveu porta no caminho.
    /// Sem NAT — que é justamente onde a pergunta "o firewall do roteador deixa
    /// entrar?" importa — a medida é exata. Com NAT, um "chegou" continua sendo
    /// prova, e um "não chegou" não é.
    entradas: Vec<(&'static str, bool, bool)>,
    quic: Option<bool>,
    furo: Furo,
}

impl Fatos {
    /// Se esta execução mediu alguma coisa que valha o código de saída zero.
    ///
    /// Um furo pedido que não abriu é fracasso; nenhuma resposta de nenhum ponto
    /// de encontro também é — nesse caso o comando não mediu nada, e um zero
    /// diria que mediu.
    fn mediu_alguma_coisa(&self) -> bool {
        if !matches!(self.furo, Furo::NaoTestado | Furo::Aberto) {
            return false;
        }
        self.sondagens
            .iter()
            .any(|sondagem| sondagem.respostas.iter().any(|r| r.visto.is_some()))
    }
}

/// Se a linha de comando pediu este diagnóstico.
#[must_use]
pub fn pedido(argumentos: &[String]) -> bool {
    argumentos.iter().any(|argumento| argumento == "--rede")
}

/// Faz o diagnóstico, imprime o relatório e devolve o código de saída.
///
/// `argumentos` são os da linha de comando **sem** o nome do programa.
///
/// Abre laço de eventos próprio, num fio próprio, porque quem chama já está
/// dentro do `#[tokio::main]` do `plug` e abrir um laço dentro do outro entra em
/// pânico.
#[must_use]
pub fn rodar(argumentos: &[String]) -> ExitCode {
    if conduzir(argumentos) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// O mesmo que [`rodar`], já terminando o processo.
///
/// É a forma de uma linha para um `main` que devolve `Result<()>` e não
/// `ExitCode`.
pub fn rodar_e_sair(argumentos: &[String]) -> ! {
    std::process::exit(i32::from(!conduzir(argumentos)))
}

/// A execução inteira, reduzida a "deu certo ou não".
fn conduzir(argumentos: &[String]) -> bool {
    let plano = match ler_plano(argumentos) {
        Ok(plano) => plano,
        Err(queixa) => {
            eprintln!("plug --rede: {queixa}");
            return false;
        }
    };
    std::thread::scope(|escopo| escopo.spawn(|| executar(&plano)).join().unwrap_or(false))
}

/// O laço de eventos e o que roda dentro dele.
fn executar(plano: &Plano) -> bool {
    let Ok(laco) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    else {
        eprintln!("plug --rede: não deu para abrir o laço de eventos");
        return false;
    };
    laco.block_on(async {
        let achados = interfaces::descobrir();
        let fatos = levantar(plano, &achados, |texto| println!("{texto}")).await;
        print!("{}", relatorio(&fatos));
        fatos.mediu_alguma_coisa()
    })
}

/// Lê a linha de comando.
///
/// `--ponto <alvo>` é repetível, e é o que destrava a classificação de NAT: com
/// dois pontos de encontro há dois pontos de vista, e sem eles a saída diz
/// `desconhecido`. O ponto do ambiente (`$SEELE_ENCONTRO`, ou o padrão) entra
/// sempre primeiro, a menos que ele esteja desligado.
fn ler_plano(argumentos: &[String]) -> Result<Plano, String> {
    // O do ambiente entra **antes** do laço, e não depois: é ele que o
    // `--ponto` de quem seguiu o conselho da linha do NAT vai repetir, e um
    // alvo repetido tem de ser recusado em voz alta em todas as portas por onde
    // ele entra. Deduplicar este em silêncio deixava a pessoa achando que tinha
    // dado o segundo ponto de vista que a saída acabara de pedir.
    let mut pontos: Vec<String> = ponto_do_ambiente().into_iter().collect();
    let mut esperar = false;
    let mut bilhete: Option<Bilhete> = None;
    let mut resto = argumentos.iter();

    while let Some(argumento) = resto.next() {
        match argumento.as_str() {
            // A bandeira que trouxe até aqui.
            "--rede" => {}
            "--esperar" => esperar = true,
            "--ponto" => {
                let Some(alvo) = resto.next() else {
                    return Err("--ponto precisa do endereço de um ponto de encontro".to_owned());
                };
                // Recusado, e não ignorado em silêncio: quem escreveu o mesmo
                // alvo duas vezes acha que deu um segundo ponto de vista, e a
                // linha do NAT ia devolver `cone` — o único veredicto que manda
                // continuar tentando — a partir de um ponto de vista só.
                if ja_pedido(&pontos, alvo) {
                    return Err(format!(
                        "{alvo} foi pedido duas vezes. Dois pedidos ao mesmo ponto de encontro \
                         não são dois pontos de vista, e classificar NAT com eles seria inventar \
                         um. Suba um segundo ponto de encontro e aponte para ele."
                    ));
                }
                pontos.push(alvo.clone());
            }
            outro if outro.starts_with('-') => return Err(format!("opção desconhecida: {outro}")),
            texto => {
                let cru = texto.strip_prefix("enc=").unwrap_or(texto);
                let lido = Bilhete::ler(cru).map_err(|erro| format!("bilhete inválido: {erro}"))?;
                bilhete = Some(lido);
            }
        }
    }

    // Quem tem o bilhete precisa falar com o ponto de encontro **dele**, mesmo
    // que seja outro: é de lá que o aviso sai para o lado que espera. Este é o
    // único que some em silêncio quando coincide, e por um motivo: ele não foi
    // digitado como segundo ponto de vista — veio dentro de um link.
    if let Some(bilhete) = bilhete.as_ref() {
        if !ja_pedido(&pontos, &bilhete.ponto) {
            pontos.push(bilhete.ponto.clone());
        }
    }

    Ok(Plano {
        pontos,
        esperar,
        bilhete,
        prazo: PRAZO,
        prazo_do_par: PRAZO_DO_PAR,
        quic: true,
    })
}

/// Se este alvo já está na lista, comparado sem diferenciar maiúsculas.
///
/// Comparação de **texto**, e ela é o que dá para fazer aqui: dois nomes DNS
/// diferentes do mesmo serviço passam, e é por isso que o cabeçalho do módulo
/// pede pontos de encontro em máquinas diferentes. O que esta função impede é o
/// caso que a própria saída convida a cometer — a linha do NAT diz `use --ponto
/// para dar um segundo`, e colar ali o endereço que já estava no ambiente
/// produziria dois "pontos de vista" que sempre concordam.
fn ja_pedido(pontos: &[String], alvo: &str) -> bool {
    pontos.iter().any(|ja| ja.eq_ignore_ascii_case(alvo.trim()))
}

/// O ponto de encontro que o ambiente pediu, ou `None` se pediu para não haver.
fn ponto_do_ambiente() -> Option<String> {
    let escolhido = std::env::var(VARIAVEL).unwrap_or_else(|_| PONTO_PADRAO.to_owned());
    let escolhido = escolhido.trim();
    if escolhido.is_empty() || escolhido.eq_ignore_ascii_case("nao") {
        return None;
    }
    Some(escolhido.to_owned())
}

/// Mede tudo. `anunciar` recebe o bilhete do modo par assim que ele existe.
async fn levantar<A>(plano: &Plano, achados: &[Achado], anunciar: A) -> Fatos
where
    A: FnOnce(&str),
{
    // **Só a placa de rede.** Um endereço de túnel é o que a linha `daqui`
    // desqualifica em voz alta, e tratá-lo como seu aqui faria a linha do NAT
    // dizer «não há NAT no caminho» sobre o endereço que ela mesma acabou de
    // desqualificar — e faria a entrada de fora chamar de conclusivo um
    // datagrama que o ponto de encontro nem chega a mandar, porque um
    // `100.64/10` não é destino que um `LEVE` alcance. É o defeito de campo que
    // originou o ADR 0022, voltando por outra porta.
    let meus: Vec<IpAddr> = achados
        .iter()
        .filter(|achado| achado.origem == Origem::Fisica)
        .map(|achado| achado.ip)
        .collect();
    let Some(mensageiro) = abrir_socket() else {
        return Fatos {
            achados: achados.to_vec(),
            meus,
            sondagens: Vec::new(),
            nat: Nat::Desconhecido,
            entradas: Vec::new(),
            quic: None,
            furo: Furo::NaoTestado,
        };
    };

    let mut sondagens: Vec<Sondagem> = Vec::new();
    for (indice, ponto) in plano.pontos.iter().enumerate() {
        sondagens.push(sondar(&mensageiro, ponto, indice, plano.prazo).await);
    }

    let nat = classificar_nat(&mapeamentos_a_comparar(&sondagens), &meus);
    // Antes da entrada de fora e do QUIC: do outro lado tem alguém esperando o
    // bilhete, e ele não pode chegar depois de dois prazos de rede.
    let furo = modo_par(plano, &mensageiro, &sondagens, anunciar).await;
    let entradas = medir_entrada_de_fora(&mensageiro, &sondagens, &meus, plano.prazo).await;
    let quic = if plano.quic {
        Some(quic_sobe().await)
    } else {
        None
    };

    Fatos {
        achados: achados.to_vec(),
        meus,
        sondagens,
        nat,
        entradas,
        quic,
        furo,
    }
}

/// Um mapeamento por ponto de encontro, todos da mesma família.
///
/// IPv4 primeiro porque é lá que existe NAT a classificar; o IPv6 entra só
/// quando não houve IPv4 nenhum, e aí a resposta interessante é justamente
/// [`Nat::Nenhum`].
fn mapeamentos_a_comparar(sondagens: &[Sondagem]) -> Vec<SocketAddr> {
    for quatro in [true, false] {
        let vistos: Vec<SocketAddr> = sondagens
            .iter()
            .filter_map(|sondagem| {
                sondagem
                    .respostas
                    .iter()
                    .filter_map(|resposta| resposta.visto)
                    .find(|visto| visto.is_ipv4() == quatro)
            })
            .collect();
        if !vistos.is_empty() {
            return vistos;
        }
    }
    Vec::new()
}

/// Pergunta `ONDE` a um ponto de encontro, uma vez por família dele.
async fn sondar(
    socket: &tokio::net::UdpSocket,
    ponto: &str,
    indice: usize,
    prazo: Duration,
) -> Sondagem {
    let mut respostas: Vec<Resposta> = Vec::new();
    for (ordem, alvo) in resolver(ponto).await.into_iter().enumerate() {
        // Uma marca por pergunta: sem isso, uma resposta atrasada do ponto
        // anterior seria contada como resposta deste.
        let visto = match marca_numerada("p", indice * 10 + ordem) {
            Some(marca) => perguntar_onde(socket, alvo, &marca, prazo).await,
            None => None,
        };
        respostas.push(Resposta { alvo, visto });
    }
    Sondagem {
        ponto: ponto.to_owned(),
        respostas,
    }
}

/// Onde um ponto de encontro atende — no máximo um endereço por família.
async fn resolver(ponto: &str) -> Vec<SocketAddr> {
    // O mesmo `Bilhete` que lê o endereço do link lê este: a porta padrão de um
    // ponto de encontro não é a de um servidor, e essa regra mora em um lugar só.
    let Ok(bilhete) = Bilhete::novo(ponto, "0.0.0.0:0") else {
        return Vec::new();
    };
    let Ok(alvo) = bilhete.ponto() else {
        return Vec::new();
    };
    let Ok(achados) = tokio::net::lookup_host((alvo.maquina, alvo.porta)).await else {
        return Vec::new();
    };
    let mut um_por_familia: Vec<SocketAddr> = Vec::new();
    for endereco in achados {
        if !um_por_familia
            .iter()
            .any(|ja| ja.is_ipv4() == endereco.is_ipv4())
        {
            um_por_familia.push(endereco);
        }
    }
    um_por_familia
}

/// Uma marca alfanumérica própria de cada pergunta.
fn marca_numerada(prefixo: &str, numero: usize) -> Option<Marca> {
    Marca::nova(&format!("{prefixo}{numero}"))
}

/// "De onde você vê que este pacote veio?", com prazo e repetição.
async fn perguntar_onde(
    socket: &tokio::net::UdpSocket,
    alvo: SocketAddr,
    marca: &Marca,
    prazo: Duration,
) -> Option<SocketAddr> {
    let pergunta = protocolo::onde(marca);
    let destino = mapear(alvo, socket);
    let mut balde = [0_u8; protocolo::TAMANHO];
    let trabalho = async {
        loop {
            if socket.send_to(&pergunta, destino).await.is_err() {
                return None;
            }
            let ate = tokio::time::Instant::now() + REPETICAO;
            loop {
                let Ok(chegada) = tokio::time::timeout_at(ate, socket.recv_from(&mut balde)).await
                else {
                    break;
                };
                let Ok((lidos, _)) = chegada else {
                    return None;
                };
                let Some(pedaco) = balde.get(..lidos) else {
                    continue;
                };
                if let Some((voltou, visto)) = protocolo::ler_aqui(pedaco) {
                    if &voltou == marca {
                        return Some(desmapear(visto));
                    }
                }
            }
        }
    };
    tokio::time::timeout(prazo, trabalho).await.ok().flatten()
}

/// Mede, por família, se entrada de fora chega mesmo.
///
/// O *ouvinte* é um socket que em toda a execução não manda um byte para lugar
/// nenhum: é essa a propriedade que faz o datagrama que chega nele ser **não
/// solicitado**, e é ela que separa este teste de um `ONDE` comum, cuja resposta
/// qualquer NAT deixa entrar por ser resposta.
async fn medir_entrada_de_fora(
    mensageiro: &tokio::net::UdpSocket,
    sondagens: &[Sondagem],
    meus: &[IpAddr],
    prazo: Duration,
) -> Vec<(&'static str, bool, bool)> {
    let Some(ouvinte) = abrir_socket() else {
        return Vec::new();
    };
    let Ok(local) = ouvinte.local_addr() else {
        return Vec::new();
    };
    let mut medidas: Vec<(&'static str, bool, bool)> = Vec::new();
    for (ordem, quatro) in [(0_usize, true), (1_usize, false)] {
        let Some((alvo, visto)) = primeiro_par(sondagens, quatro) else {
            continue;
        };
        let Some(marca) = marca_numerada("ef", ordem) else {
            continue;
        };
        // O endereço global observado, com a porta do ouvinte: a "porta alta"
        // de um socket que nunca falou com o ponto de encontro.
        let meu_global = SocketAddr::new(visto.ip(), local.port());
        let chegou =
            entrada_de_fora_chega(mensageiro, &ouvinte, alvo, meu_global, &marca, prazo).await;
        // Sem NAT a porta do ouvinte **é** a porta pública dele, e o silêncio
        // vira medida; com NAT, o silêncio pode ser só a porta reescrita.
        let conclusivo = meus.contains(&visto.ip());
        medidas.push((if quatro { "IPv4" } else { "IPv6" }, chegou, conclusivo));
    }
    medidas
}

/// O primeiro par (ponto de encontro, endereço observado) de uma família.
fn primeiro_par(sondagens: &[Sondagem], quatro: bool) -> Option<(SocketAddr, SocketAddr)> {
    sondagens.iter().find_map(|sondagem| {
        sondagem.respostas.iter().find_map(|resposta| {
            let visto = resposta.visto?;
            (visto.is_ipv4() == quatro).then_some((resposta.alvo, visto))
        })
    })
}

/// Se entrada de fora chega mesmo, e não só "tem chance de chegar".
///
/// `LEVE <meu próprio endereço global:porta alta>` faz o ponto de encontro
/// mandar um datagrama não solicitado ao `ouvinte`, que nunca falou com ele. Se
/// chega, entrada de fora funciona de verdade.
///
/// Limite honesto, e ele fica na saída: prova que 96 bytes daquela origem
/// chegaram àquela porta, não que o aperto de mão QUIC sobe.
async fn entrada_de_fora_chega(
    mensageiro: &tokio::net::UdpSocket,
    ouvinte: &tokio::net::UdpSocket,
    ponto: SocketAddr,
    meu_global: SocketAddr,
    marca: &Marca,
    prazo: Duration,
) -> bool {
    let datagrama = protocolo::leve(meu_global, marca);
    if mensageiro
        .send_to(&datagrama, mapear(ponto, mensageiro))
        .await
        .is_err()
    {
        return false;
    }
    let mut balde = [0_u8; protocolo::TAMANHO];
    let trabalho = async {
        loop {
            let Ok((lidos, _)) = ouvinte.recv_from(&mut balde).await else {
                return false;
            };
            let Some(pedaco) = balde.get(..lidos) else {
                continue;
            };
            if let Some((voltou, _)) = protocolo::ler_aqui(pedaco) {
                if &voltou == marca {
                    return true;
                }
            }
        }
    };
    tokio::time::timeout(prazo, trabalho).await.unwrap_or(false)
}

/// O degrau 4 inteiro, sem servidor atrás. Fora dele, `Furo::NaoTestado`.
async fn modo_par<A>(
    plano: &Plano,
    socket: &tokio::net::UdpSocket,
    sondagens: &[Sondagem],
    anunciar: A,
) -> Furo
where
    A: FnOnce(&str),
{
    if plano.esperar {
        let Some(bilhete) = bilhete_para_anunciar(sondagens) else {
            anunciar(
                "plug --rede --esperar: nenhum ponto de encontro respondeu, e sem isso não há \
                 bilhete a dar",
            );
            return Furo::SemBilheteAAnunciar;
        };
        anunciar(&format!("enc={bilhete}"));
        return esperar_o_par(socket, plano.prazo_do_par).await;
    }
    let Some(bilhete) = plano.bilhete.as_ref() else {
        return Furo::NaoTestado;
    };
    let Ok(aviso) = bilhete.aviso() else {
        return Furo::BilheteSemEndereco;
    };
    let Some(ponto) = resolver(&bilhete.ponto).await.into_iter().next() else {
        return Furo::PontoDoBilheteNaoResolve;
    };
    bater_no_par(socket, ponto, aviso, plano.prazo_do_par).await
}

/// O bilhete que o lado que espera imprime: o ponto de encontro e o endereço
/// que ele viu.
fn bilhete_para_anunciar(sondagens: &[Sondagem]) -> Option<Bilhete> {
    sondagens.iter().find_map(|sondagem| {
        sondagem.respostas.iter().find_map(|resposta| {
            let visto = resposta.visto?;
            Bilhete::novo(sondagem.ponto.clone(), visto.to_string()).ok()
        })
    })
}

/// O lado que espera: ouve o aviso, manda a `ida` para o endereço que ele traz,
/// e só diz que abriu quando a `volta` chega de lá.
///
/// A `volta` é a prova dos dois sentidos deste lado: ela só existe se a `ida`
/// chegou. O `pronto` que ele manda em seguida não é para ele — é a prova do
/// outro lado, e a carência no fim existe para que perdê-lo não faça as duas
/// saídas discordarem sobre a mesma tentativa.
async fn esperar_o_par(socket: &tokio::net::UdpSocket, prazo: Duration) -> Furo {
    let (Some(par), Some(ida), Some(volta), Some(pronto)) = (
        Marca::nova(MARCA_DO_PAR),
        Marca::nova(MARCA_IDA),
        Marca::nova(MARCA_VOLTA),
        Marca::nova(MARCA_PRONTO),
    ) else {
        return Furo::NaoAbriu;
    };
    let bytes_da_ida = protocolo::furo(&ida);
    let bytes_da_volta = protocolo::furo(&volta);
    let bytes_do_pronto = protocolo::furo(&pronto);
    let mut balde = [0_u8; protocolo::TAMANHO];
    let trabalho = async {
        loop {
            let Ok((lidos, de)) = socket.recv_from(&mut balde).await else {
                return Furo::NaoAbriu;
            };
            let Some(pedaco) = balde.get(..lidos) else {
                continue;
            };
            if pedaco == bytes_da_volta {
                let _ = socket.send_to(&bytes_do_pronto, de).await;
                confirmar_enquanto_o_par_repete(socket, &bytes_da_volta, &bytes_do_pronto).await;
                return Furo::Aberto;
            }
            if let Some((voltou, endereco)) = protocolo::ler_aqui(pedaco) {
                if voltou == par {
                    let _ = socket
                        .send_to(&bytes_da_ida, mapear(endereco, socket))
                        .await;
                }
            }
        }
    };
    tokio::time::timeout(prazo, trabalho)
        .await
        .unwrap_or(Furo::NaoAbriu)
}

/// A carência depois do primeiro `pronto`.
///
/// O outro lado repete a `volta` até ver um `pronto`; se o primeiro se perder,
/// sem isto ele esgotaria o prazo e imprimiria `FALHOU` enquanto este lado já
/// tinha impresso «abriu». Duas repetições cobrem o caso, e custam esse tempo só
/// no caminho que já deu certo.
async fn confirmar_enquanto_o_par_repete(
    socket: &tokio::net::UdpSocket,
    bytes_da_volta: &[u8],
    bytes_do_pronto: &[u8],
) {
    let ate = tokio::time::Instant::now() + REPETICAO * 2;
    let mut eco = [0_u8; protocolo::TAMANHO];
    loop {
        let Ok(chegada) = tokio::time::timeout_at(ate, socket.recv_from(&mut eco)).await else {
            return;
        };
        let Ok((lidos, quem)) = chegada else {
            return;
        };
        if eco.get(..lidos) == Some(bytes_da_volta) {
            let _ = socket.send_to(bytes_do_pronto, quem).await;
        }
    }
}

/// O lado que tem o bilhete: avisa o ponto de encontro, devolve a `volta` quando
/// a `ida` chega, e só diz que abriu quando o `pronto` confirma que ela chegou.
///
/// A versão anterior devolvia o eco e declarava sucesso **na mesma linha**, sem
/// confirmar nada: media um sentido e imprimia dois. O `pronto` é o que fecha —
/// ele só existe se a `volta` atravessou.
async fn bater_no_par(
    socket: &tokio::net::UdpSocket,
    ponto: SocketAddr,
    aviso: SocketAddr,
    prazo: Duration,
) -> Furo {
    let (Some(par), Some(ida), Some(volta), Some(pronto)) = (
        Marca::nova(MARCA_DO_PAR),
        Marca::nova(MARCA_IDA),
        Marca::nova(MARCA_VOLTA),
        Marca::nova(MARCA_PRONTO),
    ) else {
        return Furo::NaoAbriu;
    };
    let aviso_datagrama = protocolo::leve(aviso, &par);
    let bytes_da_ida = protocolo::furo(&ida);
    let bytes_da_volta = protocolo::furo(&volta);
    let bytes_do_pronto = protocolo::furo(&pronto);
    let ponto_mapeado = mapear(ponto, socket);
    let mut balde = [0_u8; protocolo::TAMANHO];
    let mut caminho_do_par: Option<SocketAddr> = None;
    let trabalho = async {
        loop {
            // Antes da `ida` chegar, o que sai é o aviso ao ponto de encontro;
            // depois dela, é a `volta`, repetida até o `pronto` confirmar.
            let saiu = match caminho_do_par {
                None => socket.send_to(&aviso_datagrama, ponto_mapeado).await,
                Some(caminho) => socket.send_to(&bytes_da_volta, caminho).await,
            };
            if saiu.is_err() {
                return Furo::NaoAbriu;
            }
            let ate = tokio::time::Instant::now() + REPETICAO;
            loop {
                let Ok(chegada) = tokio::time::timeout_at(ate, socket.recv_from(&mut balde)).await
                else {
                    break;
                };
                let Ok((lidos, de)) = chegada else {
                    return Furo::NaoAbriu;
                };
                let Some(pedaco) = balde.get(..lidos) else {
                    continue;
                };
                if pedaco == bytes_do_pronto {
                    return Furo::Aberto;
                }
                if pedaco == bytes_da_ida {
                    caminho_do_par = Some(de);
                    // Já, e não na próxima rodada: a `ida` acabou de provar que
                    // o caminho está aberto, e esperar 300 ms para responder é
                    // 300 ms em que um mapeamento de NAT pode fechar.
                    let _ = socket.send_to(&bytes_da_volta, de).await;
                }
            }
        }
    };
    tokio::time::timeout(prazo, trabalho)
        .await
        .unwrap_or(Furo::NaoAbriu)
}

/// Se o aperto de mão QUIC sobe nesta máquina, medido no laço local.
///
/// Um servidor em memória no `127.0.0.1` e um cliente batendo nele. Separa
/// "a máquina não fala QUIC" — antivírus, política de grupo, biblioteca de
/// criptografia recusada — de "a rede não deixa", que é tudo o mais nesta saída.
async fn quic_sobe() -> bool {
    let config = seele_server::ServerConfig {
        name: "diagnóstico".to_owned(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: seele_server::persistence::Location::Memory,
        ..seele_server::ServerConfig::default()
    };
    let Ok(servidor) = seele_server::Daemon::bind(config).await else {
        return false;
    };
    let servidor = Arc::new(servidor);
    let Ok(endereco) = servidor.local_addr() else {
        return false;
    };
    let atendendo = Arc::clone(&servidor);
    let tarefa = tokio::spawn(async move {
        let _ = atendendo.run().await;
    });

    let chave = seele_core::SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let pinos: Arc<dyn seele_core::PinStore> = Arc::new(seele_core::MemoryPinStore::new());
    let subiu = tokio::time::timeout(
        Duration::from_secs(5),
        seele_core::Client::connect(
            endereco,
            "localhost",
            "diagnostico",
            "diagnostico",
            &chave,
            pinos,
            None,
        ),
    )
    .await
    .is_ok_and(|resultado| resultado.is_ok());

    servidor.shutdown();
    tarefa.abort();
    subiu
}

/// Um socket que alcança as duas famílias, como o do QUIC.
///
/// Pela [`seele_server::alcance::abrir_escuta`] e não por um `bind` cru, porque
/// é ela que escreve **e confere** o `IPV6_V6ONLY` — a opção cujo padrão muda de
/// sistema para sistema, e que o degrau 2 do ADR 0022 mediu apanhando.
fn abrir_socket() -> Option<tokio::net::UdpSocket> {
    let (socket, _pilha) =
        seele_server::alcance::abrir_escuta(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).ok()?;
    socket.set_nonblocking(true).ok()?;
    tokio::net::UdpSocket::from_std(socket).ok()
}

/// Um destino IPv4 escrito como um socket de pilha dupla o entende.
fn mapear(destino: SocketAddr, socket: &tokio::net::UdpSocket) -> SocketAddr {
    let local_e_seis = socket.local_addr().is_ok_and(|local| local.is_ipv6());
    match (local_e_seis, destino.ip()) {
        (true, IpAddr::V4(quatro)) => {
            SocketAddr::new(quatro.to_ipv6_mapped().into(), destino.port())
        }
        _ => destino,
    }
}

/// O caminho inverso: `::ffff:a.b.c.d` volta a ser `a.b.c.d`.
///
/// Um ponto de encontro de pilha dupla relata a origem na forma mapeada, e sem
/// isto o endereço que ele viu nunca bateria com um endereço desta máquina —
/// [`Nat::Nenhum`] deixaria de existir em metade das máquinas.
fn desmapear(endereco: SocketAddr) -> SocketAddr {
    match endereco.ip() {
        IpAddr::V6(seis) => match seis.to_ipv4_mapped() {
            Some(quatro) => SocketAddr::new(IpAddr::V4(quatro), endereco.port()),
            None => endereco,
        },
        IpAddr::V4(_) => endereco,
    }
}

/// O relatório inteiro, uma linha por fato.
fn relatorio(fatos: &Fatos) -> String {
    let mut texto = String::new();
    texto.push_str(TITULO);
    texto.push('\n');
    texto.push_str(&"─".repeat(TITULO.chars().count()));
    texto.push('\n');

    escrever_daqui(&mut texto, fatos);
    escrever_udp(&mut texto, fatos);
    escrever_visto(&mut texto, fatos);
    escrever_entrada(&mut texto, fatos);
    escrever_nat(&mut texto, fatos);
    escrever_pontos(&mut texto, fatos);
    escrever_firewall(&mut texto);
    escrever_quic(&mut texto, fatos);
    escrever_furo(&mut texto, fatos);
    texto
}

/// Uma linha do relatório, em duas colunas.
fn linha(texto: &mut String, rotulo: &str, valor: &str) {
    let _ = writeln!(texto, "{rotulo:<ROTULO$}{valor}");
}

/// Os endereços desta máquina, separando a placa de rede do túnel — que é o
/// defeito de campo que originou o ADR 0022.
fn escrever_daqui(texto: &mut String, fatos: &Fatos) {
    let grupos = [
        (Origem::Fisica, "(placa de rede)"),
        (Origem::Tunel, "(túnel — não vale como endereço seu)"),
        (Origem::Virtual, "(ponte — não sai desta máquina)"),
    ];
    let mut rotulo = "daqui";
    for (origem, nota) in grupos {
        let enderecos: Vec<String> = fatos
            .achados
            .iter()
            .filter(|achado| achado.origem == origem)
            .map(|achado| achado.ip.to_string())
            .collect();
        if enderecos.is_empty() {
            continue;
        }
        linha(texto, rotulo, &format!("{} {nota}", enderecos.join(" · ")));
        rotulo = "";
    }
    if rotulo == "daqui" {
        linha(texto, "daqui", "nenhum endereço utilizável nesta máquina");
    }
}

/// A primeira bifurcação: se UDP sai daqui.
fn escrever_udp(texto: &mut String, fatos: &Fatos) {
    let saiu = fatos
        .sondagens
        .iter()
        .any(|sondagem| sondagem.respostas.iter().any(|r| r.visto.is_some()));
    let tentou = fatos
        .sondagens
        .iter()
        .any(|sondagem| !sondagem.respostas.is_empty());
    let valor = if saiu {
        "sim"
    } else if tentou {
        "não — nenhum ponto de encontro respondeu"
    } else {
        "não deu para saber — nenhum ponto de encontro foi alcançado"
    };
    linha(texto, "UDP sai", valor);
}

/// De onde o mundo vê esta máquina falar.
fn escrever_visto(texto: &mut String, fatos: &Fatos) {
    let mut vistos: Vec<SocketAddr> = Vec::new();
    for sondagem in &fatos.sondagens {
        for resposta in &sondagem.respostas {
            if let Some(visto) = resposta.visto {
                if !vistos.contains(&visto) {
                    vistos.push(visto);
                }
            }
        }
    }
    if vistos.is_empty() {
        linha(
            texto,
            "visto de fora",
            "nada voltou: ninguém disse de onde esta máquina fala",
        );
        return;
    }
    let mut rotulo = "visto de fora";
    for visto in vistos {
        let nota = if fatos.meus.contains(&visto.ip()) {
            "— é um endereço desta máquina: não há NAT no caminho"
        } else {
            "— não é endereço seu: há NAT no caminho"
        };
        linha(texto, rotulo, &format!("{visto} {nota}"));
        rotulo = "";
    }
}

/// A única linha que, até hoje, ninguém conseguia responder.
fn escrever_entrada(texto: &mut String, fatos: &Fatos) {
    if fatos.entradas.is_empty() {
        linha(
            texto,
            "entrada de fora",
            "não testado — nenhum endereço observado para apontar",
        );
        return;
    }
    let partes: Vec<String> = fatos
        .entradas
        .iter()
        .map(|(familia, chega, conclusivo)| match (chega, conclusivo) {
            (true, _) => format!("{familia} chega"),
            // Afirmativo, e não a ausência da ressalva: é a única resposta
            // definitiva que esta linha consegue dar, e ela não pode depender de
            // quem lê reparar no que **não** está escrito.
            (false, true) => format!(
                "{familia} não chega — conclusivo: o endereço observado é seu, então a porta do \
                 ouvinte é a porta pública dele"
            ),
            // Um silêncio que pode ser a porta reescrita não é o mesmo que um
            // firewall que recusa, e chamar os dois de "não chega" seco seria a
            // mentira confiante que este comando existe para não produzir.
            (false, false) => format!("{familia} não chega — e com NAT no caminho isso não é conclusivo: a porta do ouvinte pode ter sido reescrita"),
        })
        .collect();
    linha(texto, "entrada de fora", &partes.join(" · "));
    // O limite honesto, e ele fica na saída e não só no código.
    linha(
        texto,
        "",
        "prova que 96 bytes daquela origem chegaram àquela porta,",
    );
    linha(texto, "", "e não que o aperto de mão QUIC sobe");
}

/// Quando parar de tentar — ou que não dá para saber.
fn escrever_nat(texto: &mut String, fatos: &Fatos) {
    let responderam = fatos
        .sondagens
        .iter()
        .filter(|sondagem| sondagem.respostas.iter().any(|r| r.visto.is_some()))
        .count();
    let valor = match fatos.nat {
        Nat::Nenhum => "não há NAT no caminho — o endereço visto é desta máquina",
        Nat::Cone => "cone — dois pontos de encontro viram o mesmo mapeamento",
        Nat::Simetrico => "simétrico — cada ponto vê um mapeamento diferente, e o furo não abre",
        Nat::Desconhecido if responderam == 0 => {
            "desconhecido — nenhum ponto de encontro respondeu"
        }
        Nat::Desconhecido => {
            "desconhecido — só um ponto de encontro respondeu; use --ponto para dar um segundo"
        }
    };
    linha(texto, "tipo de NAT", valor);
}

/// "O serviço caiu" separado de "a minha rede não deixa".
fn escrever_pontos(texto: &mut String, fatos: &Fatos) {
    if fatos.sondagens.is_empty() {
        linha(
            texto,
            "ponto de encontro",
            "nenhum — $SEELE_ENCONTRO desligou o degrau 4",
        );
        return;
    }
    let mut rotulo = "ponto de encontro";
    for sondagem in &fatos.sondagens {
        let valor = if sondagem.respostas.is_empty() {
            "o nome não resolve".to_owned()
        } else {
            let familias: Vec<&str> = sondagem
                .respostas
                .iter()
                .filter(|resposta| resposta.visto.is_some())
                .map(|resposta| {
                    if resposta.alvo.is_ipv4() {
                        "IPv4"
                    } else {
                        "IPv6"
                    }
                })
                .collect();
            match familias.len() {
                0 => "não respondeu".to_owned(),
                1 => format!("{} respondeu", familias.join(" e ")),
                _ => format!("{} responderam", familias.join(" e ")),
            }
        };
        linha(texto, rotulo, &format!("{} — {valor}", sondagem.ponto));
        rotulo = "";
    }
}

/// Se a máquina fala QUIC, independentemente da rede.
/// O firewall desta máquina, e só quando há o que dizer.
///
/// Três respostas e duas linhas: `Liberada` não vira linha nenhuma, porque uma
/// boa notícia gritada vira ruído que se aprende a ignorar — inclusive no dia em
/// que a notícia for ruim. E `NaoSei` também não vira linha, que é a regra deste
/// arquivo inteiro: sem informação, não se escreve.
///
/// Só `Barrada` fala, e ela fala com o comando pronto. É o único lugar desta
/// saída que manda a pessoa **fazer** algo, e ela é a única que sabe o que fazer
/// — criar a regra exige administrador, e o SEELE não é nem quer ser.
fn escrever_firewall(texto: &mut String) {
    use seele_server::alcance::firewall::{self, Entrada};

    if firewall::entrada_para_este_programa() != Entrada::Barrada {
        return;
    }
    let Ok(eu) = std::env::current_exe() else {
        return;
    };
    linha(
        texto,
        "firewall",
        "não há regra de entrada para este programa, e no Windows isso barra \
         conexão de fora",
    );
    linha(texto, "", &firewall::comando_para_liberar(&eu));
}

fn escrever_quic(texto: &mut String, fatos: &Fatos) {
    let valor = match fatos.quic {
        Some(true) => "sobe nesta máquina",
        Some(false) => "não sobe nesta máquina",
        None => "não testado",
    };
    linha(texto, "QUIC", valor);
}

/// O furo, que não promete nada que não mediu.
fn escrever_furo(texto: &mut String, fatos: &Fatos) {
    let valor = match fatos.furo {
        Furo::NaoTestado => "não testado: precisa de outra máquina, em outra rede",
        Furo::Aberto => {
            "abriu — 96 bytes atravessaram nos dois sentidos, e os dois lados confirmaram"
        }
        // A única causa medida, e por isso a única que diz FALHOU.
        Furo::NaoAbriu => "FALHOU — o par não chegou dentro do prazo",
        Furo::BilheteSemEndereco => {
            "não testado: o bilhete não traz um endereço para avisar, e nada foi mandado"
        }
        Furo::PontoDoBilheteNaoResolve => {
            "não testado: o ponto de encontro do bilhete não resolve, e nada foi mandado"
        }
        Furo::SemBilheteAAnunciar => {
            "não testado: nenhum ponto de encontro respondeu, e sem isso não há bilhete a dar"
        }
    };
    linha(texto, "furo", valor);
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Um ponto de encontro **de mentira**, no laço local.
    ///
    /// Ele monta a resposta com os mesmos `analisar` e `aqui` do serviço
    /// público, campo a campo — mas **não** herda `responder_em` nem a
    /// `Vizinhanca`, que é onde mora a política antiamplificação do ADR 0022.
    /// Essa política é de quem opera um ponto de encontro, e uma casca que
    /// pudesse escolhê-la deixaria de ser casca; ela é conferida pelos testes do
    /// próprio `seele-proto`, e não daqui.
    ///
    /// A diferença prática, e ela é a favor deste duplo: no laço local todo
    /// destino é privado, e o serviço público recusa refletir para lá. Um duplo
    /// que herdasse a regra não conseguiria exercitar o mecanismo numa máquina
    /// só — que é o motivo de a `Vizinhanca` existir lá, e o motivo de ela não
    /// precisar existir aqui.
    struct PontoDeTeste {
        alvo: SocketAddr,
        tarefa: tokio::task::JoinHandle<()>,
    }

    impl PontoDeTeste {
        fn fechar(self) {
            self.tarefa.abort();
        }
    }

    /// Sobe o duplo no laço local.
    ///
    /// `reflete` desligado faz dele um ponto que responde `ONDE` e ignora
    /// `LEVE`: é como se comporta a rede em que entrada de fora não chega.
    async fn ponto_de_teste(reflete: bool) -> Option<PontoDeTeste> {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok()?;
        let alvo = socket.local_addr().ok()?;
        let tarefa = tokio::spawn(async move {
            let mut balde = [0_u8; protocolo::TAMANHO];
            loop {
                let Ok((lidos, de)) = socket.recv_from(&mut balde).await else {
                    return;
                };
                let Some(pedaco) = balde.get(..lidos) else {
                    continue;
                };
                let Some(pedido) = protocolo::analisar(pedaco) else {
                    continue;
                };
                let (destino, marca) = match pedido {
                    protocolo::Pedido::Onde { marca } => (de, marca),
                    protocolo::Pedido::Leve { destino, marca } if reflete => (destino, marca),
                    protocolo::Pedido::Leve { .. } => continue,
                };
                // A origem que vai dentro do `AQUI` é sempre a que **este**
                // socket observou, e nunca um campo copiado do pedido.
                let _ = socket.send_to(&protocolo::aqui(&marca, de), destino).await;
            }
        });
        Some(PontoDeTeste { alvo, tarefa })
    }

    /// Um plano curto: prazos de teste, e sem o laço local de QUIC, que é medido
    /// no teste que é sobre ele.
    fn plano_de_teste(pontos: Vec<String>) -> Plano {
        Plano {
            pontos,
            esperar: false,
            bilhete: None,
            prazo: Duration::from_millis(500),
            prazo_do_par: Duration::from_millis(500),
            quic: false,
        }
    }

    /// Os endereços que esta máquina *diz* ter, encenados, todos de placa de
    /// rede.
    fn achados(enderecos: &[&str]) -> Vec<Achado> {
        achados_de(Origem::Fisica, enderecos)
    }

    /// O mesmo, dizendo de que tipo de interface eles vieram.
    fn achados_de(origem: Origem, enderecos: &[&str]) -> Vec<Achado> {
        enderecos
            .iter()
            .filter_map(|texto| texto.parse::<IpAddr>().ok())
            .map(|ip| Achado {
                ip,
                mascara: None,
                origem,
            })
            .collect()
    }

    /// Fatos sem medida nenhuma, para exercitar o texto do relatório.
    fn fatos_vazios() -> Fatos {
        Fatos {
            achados: Vec::new(),
            meus: Vec::new(),
            sondagens: Vec::new(),
            nat: Nat::Desconhecido,
            entradas: Vec::new(),
            quic: None,
            furo: Furo::NaoTestado,
        }
    }

    /// A linha do relatório que começa com este rótulo, sem o rótulo.
    fn linha_de(saida: &str, rotulo: &str) -> String {
        saida
            .lines()
            .find(|linha| linha.starts_with(rotulo))
            .map(|linha| linha.chars().skip(ROTULO).collect())
            .unwrap_or_default()
    }

    // ── o que a ferramenta pode afirmar ────────────────────────────────────

    #[test]
    fn o_tipo_de_nat_e_desconhecido_com_um_ponto_de_encontro_so() {
        // Classificar cone contra simétrico exige comparar o mapeamento do
        // **mesmo socket local** visto de **dois destinos diferentes**. `ONDE`
        // responde pelo socket que recebeu e `LEVE` reflete a partir do mesmo
        // socket: a origem de todo `AQUI` é `IP-do-ponto:8384`,
        // invariavelmente. Não há segundo ponto de vista, e inventar um seria a
        // mentira confiante que o ADR 0022 existe para não produzir.
        let visto = "200.100.30.40:61234".parse().ok();
        let meus = ["192.168.1.20".parse().ok()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            classificar_nat(&[visto].into_iter().flatten().collect::<Vec<_>>(), &meus),
            Nat::Desconhecido
        );
    }

    #[test]
    fn sem_nat_no_caminho_quando_o_endereco_visto_e_meu() {
        // O que uma máquina só afirma com certeza, e ainda vale: se o endereço
        // que o ponto de encontro viu é um dos endereços desta máquina, não há
        // NAT no caminho. É o degrau 1 do ADR 0022, medido em vez de deduzido.
        let visto = "45.33.32.156:61234".parse().ok();
        let meus = ["45.33.32.156".parse().ok()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            classificar_nat(&[visto].into_iter().flatten().collect::<Vec<_>>(), &meus),
            Nat::Nenhum
        );
    }

    #[test]
    fn dois_pontos_com_o_mesmo_mapeamento_sao_cone() {
        let a = "200.100.30.40:61234".parse().ok();
        let b = "200.100.30.40:61234".parse().ok();
        let meus = ["192.168.1.20".parse().ok()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let vistos = [a, b].into_iter().flatten().collect::<Vec<_>>();
        assert_eq!(classificar_nat(&vistos, &meus), Nat::Cone);
    }

    #[test]
    fn dois_pontos_com_mapeamentos_diferentes_sao_simetrico() {
        // O caso sem saída do ADR 0022, e o único que este comando consegue
        // nomear antes de a pessoa perder uma tarde tentando.
        let a = "200.100.30.40:61234".parse().ok();
        let b = "200.100.30.40:52001".parse().ok();
        let meus = ["192.168.1.20".parse().ok()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let vistos = [a, b].into_iter().flatten().collect::<Vec<_>>();
        assert_eq!(classificar_nat(&vistos, &meus), Nat::Simetrico);
    }

    // ── a fiação: o que a saída de verdade diz ─────────────────────────────

    #[tokio::test]
    async fn a_saida_diz_desconhecido_quando_so_um_ponto_respondeu() {
        // A metade que importa. `classificar_nat` é função pura e é fácil
        // deixá-la verde sozinha; o que decide se este comando vale é a saída
        // **de verdade** dizer `desconhecido` quando só houve um ponto de vista.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        let fatos = levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await;
        let saida = relatorio(&fatos);
        ponto.fechar();

        let linha = linha_de(&saida, "tipo de NAT");
        assert!(
            linha.starts_with("desconhecido — só um ponto de encontro respondeu"),
            "a linha do tipo de NAT foi «{linha}»\n{saida}"
        );
    }

    #[tokio::test]
    async fn dois_pontos_configurados_mas_um_calado_ainda_e_desconhecido() {
        // O guarda do teste de cima, contra a forma mais fácil de errar isto:
        // classificar por **quantos pontos foram pedidos** em vez de por
        // quantos responderam. `127.0.0.1:1` nunca responde — e é porta baixa,
        // então nem um ponto de encontro de verdade ali refletiria.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let plano = plano_de_teste(vec![ponto.alvo.to_string(), "127.0.0.1:1".to_owned()]);
        let fatos = levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await;
        let saida = relatorio(&fatos);
        ponto.fechar();

        assert!(
            linha_de(&saida, "tipo de NAT").starts_with("desconhecido"),
            "dois pontos pedidos e um calado não podem virar classificação\n{saida}"
        );
        assert!(
            saida.contains("não respondeu"),
            "a linha do ponto calado sumiu da saída\n{saida}"
        );
    }

    #[tokio::test]
    async fn a_saida_diz_cone_quando_dois_pontos_viram_o_mesmo_mapeamento() {
        // Dois pontos de vista, e o mesmo mapeamento nos dois: é a única
        // combinação em que este comando pode afirmar cone. No laço local o
        // mapeamento é o próprio socket, então os dois veem igual.
        let (Some(um), Some(outro)) = (ponto_de_teste(true).await, ponto_de_teste(true).await)
        else {
            panic!("os pontos de encontro de teste não subiram");
        };
        let plano = plano_de_teste(vec![um.alvo.to_string(), outro.alvo.to_string()]);
        let fatos = levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await;
        let saida = relatorio(&fatos);
        um.fechar();
        outro.fechar();

        let linha = linha_de(&saida, "tipo de NAT");
        assert!(
            linha.starts_with("cone — dois pontos de encontro viram o mesmo mapeamento"),
            "a linha do tipo de NAT foi «{linha}»\n{saida}"
        );
    }

    #[tokio::test]
    async fn a_saida_diz_que_nao_ha_nat_quando_o_endereco_visto_e_desta_maquina() {
        // O único veredicto que uma máquina dá com certeza a partir de um ponto
        // de encontro só. Aqui o ponto vê `127.0.0.1`, e `127.0.0.1` é um
        // endereço desta máquina — a mesma conta que uma VPS faz com o IPv4
        // dela.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        let fatos = levantar(&plano, &achados(&["127.0.0.1"]), |_| {}).await;
        let saida = relatorio(&fatos);
        ponto.fechar();

        assert!(
            linha_de(&saida, "tipo de NAT").starts_with("não há NAT no caminho"),
            "{saida}"
        );
        assert!(
            linha_de(&saida, "visto de fora").contains("é um endereço desta máquina"),
            "{saida}"
        );
    }

    // ── a entrada não solicitada ───────────────────────────────────────────

    #[tokio::test]
    async fn a_entrada_de_fora_e_medida_e_o_limite_dela_aparece_na_saida() {
        // O único teste do projeto que transforma o "chance, e não certeza" de
        // `Degrau::alcanca_de_fora` em fato medido: o ponto de encontro manda 96
        // bytes a um socket que nunca falou com ele, e ou eles chegam ou não.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        let fatos = levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await;
        let saida = relatorio(&fatos);
        ponto.fechar();

        assert_eq!(
            fatos.entradas,
            vec![("IPv4", true, false)],
            "o datagrama não solicitado não chegou ao ouvinte\n{saida}"
        );
        assert!(
            linha_de(&saida, "entrada de fora").contains("IPv4 chega"),
            "{saida}"
        );
        assert!(
            saida.contains("prova que 96 bytes daquela origem chegaram àquela porta,")
                && saida.contains("e não que o aperto de mão QUIC sobe"),
            "o limite honesto da medida tem de estar na saída, não só no código\n{saida}"
        );
    }

    #[tokio::test]
    async fn a_entrada_de_fora_diz_que_nao_chega_quando_nao_chega() {
        // O outro lado, e o que impede a medida de ser um `true` constante: um
        // ponto que responde `ONDE` e ignora `LEVE` é exatamente a rede em que
        // sair funciona e entrar não.
        let Some(ponto) = ponto_de_teste(false).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        let fatos = levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await;
        let saida = relatorio(&fatos);
        ponto.fechar();

        assert_eq!(fatos.entradas, vec![("IPv4", false, false)], "{saida}");
        let linha = linha_de(&saida, "entrada de fora");
        assert!(linha.contains("IPv4 não chega"), "{saida}");
        assert!(
            linha.contains("não é conclusivo"),
            "com NAT no caminho, um silêncio pode ser a porta reescrita, e a \
             saída tem de dizer isso em vez de acusar o firewall\n{saida}"
        );
    }

    #[tokio::test]
    async fn sem_nat_no_caminho_um_nao_chega_e_conclusivo() {
        // O outro lado da ressalva, e o que impede a ressalva de ser um rodapé
        // constante: quando o endereço observado é desta máquina, a porta do
        // ouvinte **é** a porta pública dele, e o silêncio deixa de ser palpite.
        let Some(ponto) = ponto_de_teste(false).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        let fatos = levantar(&plano, &achados(&["127.0.0.1"]), |_| {}).await;
        let saida = relatorio(&fatos);
        ponto.fechar();

        assert_eq!(fatos.entradas, vec![("IPv4", false, true)], "{saida}");
        let linha = linha_de(&saida, "entrada de fora");
        assert!(linha.contains("IPv4 não chega"), "{saida}");
        assert!(
            linha.contains("conclusivo: o endereço observado é seu"),
            "a única resposta definitiva desta linha tem de se declarar, e não \
             ficar implícita na ausência da ressalva\n{saida}"
        );
        assert!(
            !linha.contains("não é conclusivo"),
            "sem NAT no caminho a medida é exata, e a ressalva sobra\n{saida}"
        );
    }

    // ── o furo ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_linha_do_furo_diz_nao_testado_fora_do_modo_par() {
        // Furo de verdade precisa de duas máquinas em redes diferentes. Uma
        // execução solitária que dissesse `FALHOU` estaria afirmando ausência
        // sobre um caminho que ela nunca exercitou.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        let saida = relatorio(&levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await);
        ponto.fechar();

        assert_eq!(
            linha_de(&saida, "furo"),
            "não testado: precisa de outra máquina, em outra rede",
            "{saida}"
        );
        assert!(
            !saida.contains("FALHOU"),
            "fora do modo par a saída não pode conter FALHOU\n{saida}"
        );
    }

    #[tokio::test]
    async fn o_modo_par_mede_o_furo_de_verdade() {
        // O degrau 4 inteiro, sem servidor atrás: um lado espera e anuncia o
        // bilhete, o outro avisa o ponto de encontro, o aviso chega ao primeiro,
        // ele fura para o endereço que veio no aviso, e os dois só afirmam
        // «abriu» depois de os 96 bytes atravessarem **nos dois sentidos**.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let alvo = ponto.alvo.to_string();
        let (emissor, receptor) = tokio::sync::oneshot::channel::<String>();

        let alvo_de_quem_espera = alvo.clone();
        let lado_que_espera = tokio::spawn(async move {
            let plano = Plano {
                pontos: vec![alvo_de_quem_espera],
                esperar: true,
                bilhete: None,
                prazo: Duration::from_millis(500),
                prazo_do_par: Duration::from_secs(5),
                quic: false,
            };
            let seus = achados(&["192.168.1.20"]);
            let fatos = levantar(&plano, &seus, move |texto| {
                let _ = emissor.send(texto.to_owned());
            })
            .await;
            relatorio(&fatos)
        });

        let Ok(anuncio) = receptor.await else {
            panic!("o lado que espera não anunciou bilhete nenhum");
        };
        let Some(cru) = anuncio.strip_prefix("enc=") else {
            panic!("o anúncio não é um bilhete: «{anuncio}»");
        };
        let Ok(bilhete) = Bilhete::ler(cru) else {
            panic!("o bilhete anunciado não é legível: «{cru}»");
        };
        assert_eq!(
            bilhete.ponto, alvo,
            "o bilhete tem de nomear o ponto de encontro por onde o aviso vai passar"
        );

        let plano_de_quem_bate = Plano {
            pontos: vec![alvo],
            esperar: false,
            bilhete: Some(bilhete),
            prazo: Duration::from_millis(500),
            prazo_do_par: Duration::from_secs(5),
            quic: false,
        };
        let saida_de_quem_bate =
            relatorio(&levantar(&plano_de_quem_bate, &achados(&["192.168.1.20"]), |_| {}).await);
        let Ok(saida_de_quem_espera) = lado_que_espera.await else {
            panic!("o lado que espera entrou em pânico");
        };
        ponto.fechar();

        assert_eq!(
            linha_de(&saida_de_quem_bate, "furo"),
            "abriu — 96 bytes atravessaram nos dois sentidos, e os dois lados confirmaram",
            "{saida_de_quem_bate}"
        );
        assert_eq!(
            linha_de(&saida_de_quem_espera, "furo"),
            "abriu — 96 bytes atravessaram nos dois sentidos, e os dois lados confirmaram",
            "{saida_de_quem_espera}"
        );
    }

    #[tokio::test]
    async fn no_modo_par_um_furo_que_nao_abre_e_dito_como_falha() {
        // E aqui `FALHOU` é honesto, porque houve o que falhar: o bilhete manda
        // o aviso para uma porta onde não há ninguém esperando, e nenhum furo
        // volta de lá.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let alvo = ponto.alvo.to_string();
        let Ok(bilhete) = Bilhete::novo(alvo.clone(), "127.0.0.1:9") else {
            panic!("o bilhete de teste não montou");
        };
        let mut plano = plano_de_teste(vec![alvo]);
        plano.bilhete = Some(bilhete);
        let saida = relatorio(&levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await);
        ponto.fechar();

        assert_eq!(
            linha_de(&saida, "furo"),
            "FALHOU — o par não chegou dentro do prazo",
            "{saida}"
        );
    }

    // ── o resto da saída ───────────────────────────────────────────────────

    #[tokio::test]
    async fn o_quic_sobe_nesta_maquina() {
        // Separa "a máquina não fala QUIC" de "a rede não deixa", que é tudo o
        // mais nesta saída. Sem ponto de encontro nenhum: este teste é sobre o
        // laço local.
        let mut plano = plano_de_teste(Vec::new());
        plano.quic = true;
        let saida = relatorio(&levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await);
        assert_eq!(
            linha_de(&saida, "QUIC"),
            "sobe nesta máquina",
            "ou a fiação desta linha quebrou, ou esta máquina não fala QUIC — a \
             suíte de conformidade diz qual, porque ela sobe o mesmo Server sem \
             passar por aqui\n{saida}"
        );
    }

    #[test]
    fn a_linha_do_quic_tem_as_tres_respostas() {
        // O `assert_eq!` do teste de cima tem dente porque esta linha sabe dizer
        // outra coisa. Sem estes dois casos, «sobe nesta máquina» poderia ser
        // texto constante e o teste de cima continuaria verde.
        let mut fatos = fatos_vazios();
        fatos.quic = Some(false);
        assert_eq!(
            linha_de(&relatorio(&fatos), "QUIC"),
            "não sobe nesta máquina"
        );
        fatos.quic = None;
        assert_eq!(linha_de(&relatorio(&fatos), "QUIC"), "não testado");
    }

    #[test]
    fn a_linha_daqui_separa_a_placa_de_rede_do_tunel() {
        // O defeito de campo que originou o ADR 0022: um endereço de túnel
        // anunciado como se fosse o da casa. A saída tem de dizer, na cara, que
        // ele não vale como endereço seu.
        let fatos = Fatos {
            achados: vec![
                Achado {
                    ip: IpAddr::from([192, 168, 0, 30]),
                    mascara: None,
                    origem: Origem::Fisica,
                },
                Achado {
                    ip: IpAddr::from([100, 96, 0, 3]),
                    mascara: None,
                    origem: Origem::Tunel,
                },
            ],
            meus: Vec::new(),
            sondagens: Vec::new(),
            nat: Nat::Desconhecido,
            entradas: Vec::new(),
            quic: None,
            furo: Furo::NaoTestado,
        };
        let saida = relatorio(&fatos);
        assert!(
            linha_de(&saida, "daqui").starts_with("192.168.0.30 (placa de rede)"),
            "{saida}"
        );
        assert!(
            saida.contains("100.96.0.3 (túnel — não vale como endereço seu)"),
            "{saida}"
        );
    }

    #[test]
    fn um_ponto_de_encontro_a_mais_entra_pela_linha_de_comando() {
        // É o que destrava a classificação de NAT, e por isso a bandeira é
        // repetível: com dois pontos há dois pontos de vista.
        let argumentos = ["--rede", "--ponto", "outro.exemplo:8384"].map(str::to_owned);
        let Ok(plano) = ler_plano(&argumentos) else {
            panic!("a linha de comando não foi lida");
        };
        assert!(
            plano.pontos.contains(&"outro.exemplo:8384".to_owned()),
            "o ponto pedido à mão não entrou: {:?}",
            plano.pontos
        );
        assert!(!plano.esperar);
        assert!(plano.bilhete.is_none());
    }

    #[test]
    fn um_bilhete_e_lido_com_ou_sem_o_prefixo_que_o_link_usa() {
        // O bilhete é copiado de um `enc=…` de um `seele://`, ou da saída de
        // `--esperar`, que imprime com o prefixo. Recusar um dos dois seria
        // recusar o que a pessoa tem na mão.
        for texto in [
            "enc=encontro.exemplo:8384/198.51.100.7:41234",
            "encontro.exemplo:8384/198.51.100.7:41234",
        ] {
            let argumentos = ["--rede".to_owned(), texto.to_owned()];
            let Ok(plano) = ler_plano(&argumentos) else {
                panic!("«{texto}» não foi lido como bilhete");
            };
            let Some(bilhete) = plano.bilhete else {
                panic!("«{texto}» não virou bilhete");
            };
            assert_eq!(bilhete.ponto, "encontro.exemplo:8384");
            assert!(
                plano.pontos.contains(&"encontro.exemplo:8384".to_owned()),
                "o ponto do bilhete tem de ser sondado junto: {:?}",
                plano.pontos
            );
        }
    }

    #[test]
    fn uma_opcao_desconhecida_e_recusada_em_vez_de_virar_bilhete() {
        assert!(ler_plano(&["--seiladoque".to_owned()]).is_err());
        assert!(ler_plano(&["--ponto".to_owned()]).is_err());
        assert!(ler_plano(&["--rede".to_owned(), "nada disso".to_owned()]).is_err());
        // O caso que separa as duas recusas. `--seiladoque` já era recusado pelo
        // braço do bilhete, por não ter barra — então apagar o guarda das opções
        // não acendia nada. Este texto **é** um bilhete bem formado, e só o
        // guarda o recusa.
        let parece_bilhete = "-x:1/1.2.3.4:5";
        assert!(
            Bilhete::ler(parece_bilhete).is_ok(),
            "este caso só separa as duas recusas se ele for mesmo um bilhete válido"
        );
        assert!(
            ler_plano(&[parece_bilhete.to_owned()]).is_err(),
            "uma opção desconhecida que por acaso é um bilhete bem formado tem de \
             ser recusada como opção, e não aceita como bilhete"
        );
    }

    #[test]
    fn dois_pedidos_ao_mesmo_ponto_de_encontro_sao_recusados() {
        // O caminho que a **própria saída ensina**: a linha do NAT imprime
        // `use --ponto para dar um segundo`, e quem colar ali o endereço que já
        // estava na lista receberia `cone` — o único veredicto que manda
        // continuar tentando — a partir de um ponto de vista só. Dois pedidos ao
        // mesmo serviço sempre concordam, porque o mapeamento é o mesmo.
        let repetido =
            ["--ponto", "a.exemplo:8384", "--ponto", "a.exemplo:8384"].map(str::to_owned);
        assert!(
            ler_plano(&repetido).is_err(),
            "o mesmo ponto de encontro pedido duas vezes viraria dois pontos de vista"
        );
        // Maiúsculas não são um segundo serviço.
        let disfarcado =
            ["--ponto", "a.exemplo:8384", "--ponto", "A.Exemplo:8384"].map(str::to_owned);
        assert!(ler_plano(&disfarcado).is_err());

        // O caminho de verdade, e o que **rodar a ferramenta** pegou: a linha do
        // NAT diz `use --ponto para dar um segundo`, e o valor mais à mão para
        // colar ali é o que já está no ambiente. Ele tem de ser recusado como
        // qualquer outro repetido — deduplicá-lo em silêncio deixava a pessoa
        // achando que tinha resolvido o `desconhecido`.
        if let Some(ambiente) = ponto_do_ambiente() {
            let colado = ["--ponto".to_owned(), ambiente];
            assert!(
                ler_plano(&colado).is_err(),
                "repetir à mão o ponto de encontro do ambiente tem de ser recusado"
            );
        }

        // E o guarda não pode barrar o caso que ele existe para permitir.
        let dois = ["--ponto", "a.exemplo:8384", "--ponto", "b.exemplo:8384"].map(str::to_owned);
        let Ok(plano) = ler_plano(&dois) else {
            panic!("dois pontos de encontro diferentes têm de passar");
        };
        assert!(plano.pontos.contains(&"a.exemplo:8384".to_owned()));
        assert!(plano.pontos.contains(&"b.exemplo:8384".to_owned()));
    }

    #[tokio::test]
    async fn um_endereco_de_tunel_nao_conta_como_endereco_seu() {
        // O defeito de campo que originou o ADR 0022, tentando voltar por outra
        // porta: a linha `daqui` diz que um endereço de túnel «não vale como
        // endereço seu», e a linha do NAT tratava o mesmo endereço como seu.
        // Com um ponto de encontro dentro da VPN, a saída diria «não há NAT no
        // caminho» sobre o endereço que ela mesma acabou de desqualificar.
        let Some(ponto) = ponto_de_teste(false).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        // O ponto de encontro observa `127.0.0.1`, e esta máquina tem
        // `127.0.0.1` — mas por um túnel.
        let fatos = levantar(&plano, &achados_de(Origem::Tunel, &["127.0.0.1"]), |_| {}).await;
        let saida = relatorio(&fatos);
        ponto.fechar();

        assert!(
            linha_de(&saida, "tipo de NAT").starts_with("desconhecido"),
            "um endereço de túnel não pode responder «não há NAT no caminho»\n{saida}"
        );
        assert!(
            linha_de(&saida, "visto de fora").contains("não é endereço seu"),
            "{saida}"
        );
        assert_eq!(
            fatos.entradas,
            vec![("IPv4", false, false)],
            "um silêncio medido contra um endereço de túnel não é conclusivo, e \
             chamá-lo de conclusivo acusa o firewall do roteador por um datagrama \
             que o ponto de encontro nem chega a mandar\n{saida}"
        );
    }

    #[tokio::test]
    async fn um_furo_de_mao_unica_nao_e_declarado_aberto() {
        // O lado que bate recebia a ida, devolvia a volta e declarava sucesso na
        // mesma linha — media um sentido e imprimia dois. Aqui o par manda a ida
        // e some: a volta sai daqui e nada confirma que ela chegou.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let Ok(par_mudo) = tokio::net::UdpSocket::bind("127.0.0.1:0").await else {
            panic!("o par mudo não subiu");
        };
        let Ok(onde_o_par_espera) = par_mudo.local_addr() else {
            panic!("o par mudo não diz onde ligou");
        };
        let (Some(marca_do_par), Some(marca_da_ida)) =
            (Marca::nova(MARCA_DO_PAR), Marca::nova(MARCA_IDA))
        else {
            panic!("as marcas do modo par não montaram");
        };
        let so_a_ida = tokio::spawn(async move {
            let bytes_da_ida = protocolo::furo(&marca_da_ida);
            let mut balde = [0_u8; protocolo::TAMANHO];
            loop {
                let Ok((lidos, _)) = par_mudo.recv_from(&mut balde).await else {
                    return;
                };
                let Some(pedaco) = balde.get(..lidos) else {
                    continue;
                };
                if let Some((voltou, endereco)) = protocolo::ler_aqui(pedaco) {
                    if voltou == marca_do_par {
                        let _ = par_mudo.send_to(&bytes_da_ida, endereco).await;
                    }
                }
                // E nada mais: nenhum `pronto` sai daqui.
            }
        });

        let Ok(bilhete) = Bilhete::novo(ponto.alvo.to_string(), onde_o_par_espera.to_string())
        else {
            panic!("o bilhete de teste não montou");
        };
        let mut plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        plano.bilhete = Some(bilhete);
        plano.prazo_do_par = Duration::from_millis(900);
        let saida = relatorio(&levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await);
        so_a_ida.abort();
        ponto.fechar();

        assert_eq!(
            linha_de(&saida, "furo"),
            "FALHOU — o par não chegou dentro do prazo",
            "um sentido medido não pode virar «atravessaram nos dois sentidos»\n{saida}"
        );
    }

    #[tokio::test]
    async fn um_bilhete_sem_endereco_nao_vira_falhou() {
        // Nada foi mandado, então não há o que ter falhado. Dar a mesma cara a
        // isto e a um par que não chegou é reconstruir aqui dentro o defeito do
        // roteiro que esta ferramenta veio substituir.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        // Um bilhete bem formado cujo aviso é um **nome**: o `LEVE` carrega
        // endereço, e ninguém no caminho resolve nome.
        let Ok(bilhete) = Bilhete::novo(ponto.alvo.to_string(), "casa.exemplo:41234") else {
            panic!("o bilhete de teste não montou");
        };
        let mut plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        plano.bilhete = Some(bilhete);
        let saida = relatorio(&levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await);
        ponto.fechar();

        assert_eq!(
            linha_de(&saida, "furo"),
            "não testado: o bilhete não traz um endereço para avisar, e nada foi mandado",
            "{saida}"
        );
        assert!(!saida.contains("FALHOU"), "{saida}");
    }

    #[tokio::test]
    async fn um_ponto_de_bilhete_que_nao_resolve_nao_vira_falhou() {
        // O outro fracasso sem pacote nenhum. `.invalid` é reservado para não
        // resolver (RFC 2606), então isto não depende do DNS de quem roda.
        let Some(ponto) = ponto_de_teste(true).await else {
            panic!("o ponto de encontro de teste não subiu");
        };
        let Ok(bilhete) = Bilhete::novo("nao-existe-mesmo.invalid:8384", "1.2.3.4:41234") else {
            panic!("o bilhete de teste não montou");
        };
        let mut plano = plano_de_teste(vec![ponto.alvo.to_string()]);
        plano.bilhete = Some(bilhete);
        let saida = relatorio(&levantar(&plano, &achados(&["192.168.1.20"]), |_| {}).await);
        ponto.fechar();

        assert_eq!(
            linha_de(&saida, "furo"),
            "não testado: o ponto de encontro do bilhete não resolve, e nada foi mandado",
            "{saida}"
        );
        assert!(!saida.contains("FALHOU"), "{saida}");
    }

    #[tokio::test]
    async fn esperar_sem_ponto_de_encontro_nenhum_nao_vira_falhou() {
        // `--esperar` sem bilhete a dar: o outro lado nem chega a saber que
        // devia tentar.
        let mut plano = plano_de_teste(Vec::new());
        plano.esperar = true;
        let mut anunciado = String::new();
        let saida = relatorio(
            &levantar(&plano, &achados(&["192.168.1.20"]), |texto| {
                anunciado.push_str(texto);
            })
            .await,
        );
        assert_eq!(
            linha_de(&saida, "furo"),
            "não testado: nenhum ponto de encontro respondeu, e sem isso não há bilhete a dar",
            "{saida}"
        );
        assert!(!saida.contains("FALHOU"), "{saida}");
        assert!(
            !anunciado.starts_with("enc="),
            "não pode sair bilhete quando não há endereço observado para pôr nele"
        );
    }

    #[test]
    fn pedido_reconhece_a_bandeira_e_so_ela() {
        assert!(pedido(&["--rede".to_owned()]));
        assert!(pedido(&["--hospedar".to_owned(), "--rede".to_owned()]));
        assert!(!pedido(&["--hospedar".to_owned()]));
        assert!(!pedido(&[]));
    }
}
