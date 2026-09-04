# Qualidade da imagem na tela compartilhada — Plano de Implementação

> **Para quem executa com agentes:** SUB-SKILL OBRIGATÓRIA: use `superpowers:subagent-driven-development` (recomendado) ou `superpowers:executing-plans` para implementar tarefa a tarefa. Os passos usam caixas (`- [ ]`) para acompanhamento.

**Objetivo:** Fazer a imagem da tela compartilhada parar de blocar quando o teto de banda aperta — em especial quando a sala cresce —, corrigindo os três pontos onde a regra do §2 («a resolução segura, o quadro cede») ainda não vale, e declarando explicitamente o perfil H.264 nos dois codificadores de hardware.

**Arquitetura:** A decisão de quantos quadros por segundo cabem no orçamento já existe e é pura (`seele_core::tela::cadencia_para`). Ela é consultada **uma vez**, ao armar o codificador. Este plano a leva para o caminho de ajuste em tempo de execução, onde o teto de fato se move, e dá aos codificadores a porta para obedecer sem custar um quadro-chave. Nenhuma tarefa toca no transporte, no protocolo de fio ou no servidor.

**Stack:** Rust 2021, `seele-video` (OpenH264 via `shiguredo_openh264`, VideoToolbox via `objc2-video-toolbox`, Media Foundation via `windows` 0.62), `seele-core`.

**Spec:** `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`, §2 (codec e captura), §3.2 (a voz nunca cede à tela), §5 e §5.1 (as três pernas do teto).

## Restrições globais

- `unsafe_code = "forbid"` no workspace. `seele-video` é a única exceção, `deny` com `allow` local em `codec/macos.rs` e `codec/windows.rs` (ADR 0041). Nenhuma tarefa deste plano acrescenta um terceiro lugar com `unsafe`.
- `unwrap_used = "deny"`, `expect_used = "deny"` fora de testes. Em testes, os arquivos já trazem `#![allow(clippy::expect_used)]` ou `cfg_attr`.
- `missing_docs = "warn"`: todo item público novo leva doc comment.
- A regra do §2 que governa este plano: **a resolução segura, o quadro cede.** A cadência escolhida pela pessoa é sempre **teto**, nunca piso (§5).
- `PISO_DE_QUADROS = 5` (`seele_video::codec`) é o piso absoluto; `Cadencia::TODAS` é `[Q8, Q15, Q30, Q60]`.
- Comentários e nomes em português, como o resto de `seele-video` e `seele-core`.
- Comando de teste padrão do workspace: `cargo test -p <crate>`.

---

### Task 0: a régua — limiares e bits por quadro calibrados em qualidade, e o teto em 8 Mbps

**É a tarefa de maior efeito por linha alterada, e nenhuma outra depende dela.** As três constantes que decidem o que sai foram calibradas no ponto em que o OpenH264 parava de desabar, e não no ponto em que a imagem fica boa. `bits_por_quadro` diz, no próprio doc, de onde veio: *«quanto o codificador de referência gastou em cada quadro que ele decidiu entregar»* — colhido num teto de 1200 kbps onde ele **já estava jogando 16% dos quadros fora**. É o pior caso medido, usado como alvo.

Em bits por pixel, que é o que decide se sai bloco:

| no limiar de hoje | bits/quadro a 30 q/s | bits/pixel |
|---|---|---|
| 1080p a 1,5 Mbps | 50 k | 0,024 |
| 720p a 900 kbps | 30 k | 0,033 |
| tela com texto pede | — | **~0,10** |

Estão em um quarto a um terço do necessário. O efeito prático é que, assim que o caminho passa de 1,5 Mbps, o produto sobe para 1080p e entrega um 1080p borrado que ficaria melhor como um 720p nítido — que é o relato «ruim com pouca gente».

**O alvo é 0,10 bits por pixel**, e os números abaixo são essa conta, não gosto:

| | 30 q/s | 60 q/s |
|---|---|---|
| 540p (518 400 px) | 1,6 Mbps | 3,1 Mbps |
| 720p (921 600 px) | 2,8 Mbps | 5,5 Mbps |
| 1080p (2 073 600 px) | 6,2 Mbps | 12,4 Mbps |

**E o teto sobe para 8 Mbps de vídeo**, que é o pedido do produto e o que compra 1080p30 com folga (0,13 bpp) ou 720p60 com folga (0,145 bpp). Oito megabits de vídeo pedem 13,3 Mbps de caminho, porque o vídeo leva 60% — daí `TETO_DA_ESTIMATIVA_BPS = 14_000_000`.

**O que isto custa, e precisa ser dito antes de alguém se assustar:** quem via «1080p» na interface vai passar a ver «720p» nas mesmas conexões. Não é regressão — era um 1080p de 0,024 bpp. Mas o §5 manda mostrar o que está saindo, então a mudança **fica visível**, e a nota de versão tem de explicar em vez de deixar a pessoa descobrir.

**O que esta tarefa não resolve, e nenhuma medida vai resolver:** 540p num monitor de 27" mostrando código continua ilegível por mais nítido que seja. Resolução também é legibilidade, e bits por pixel não captura isso. Por isso o Passo 5 é olhar, e não só ler o PSNR.

**Arquivos:**
- Modificar: `crates/seele-core/src/tela.rs` — `TETO_ESTIMADO_PARA_1080P_BPS` (linha 494), `TETO_ESTIMADO_PARA_720P_BPS` (linha 505), `bits_por_quadro` (a partir da linha 582)
- Modificar: `crates/seele-core/src/caminho.rs` — `TETO_DA_ESTIMATIVA_BPS` (linha 264)
- Teste: `crates/seele-core/src/tela.rs`, módulo de testes (o que já usa `cadencia_para` e `resolucao_para`, a partir da linha 2505)

**Interfaces:**
- Produz: as constantes recalibradas. As Tasks 2, 3 e 6 usam os valores novos nos testes e nas medidas — quem executar fora de ordem vai ver números diferentes dos escritos lá.

- [ ] **Passo 1: escrever o teste que falha**

Em `crates/seele-core/src/tela.rs`, no módulo de testes que já importa `cadencia_para` e `resolucao_para`:

```rust
/// Quantos bits por pixel um degrau entrega no seu próprio limiar.
///
/// É a conta que decide se sai bloco, e a única que atravessa resolução e
/// cadência ao mesmo tempo: um limiar só é honesto se, exatamente em cima
/// dele, a imagem que ele promete couber nos bits que ele tem.
fn bits_por_pixel(teto_bps: u32, resolucao: Resolucao, quadros: u32) -> f64 {
    let pixels = (resolucao.largura() * resolucao.altura()) as f64;
    f64::from(teto_bps) / f64::from(quadros) / pixels
}

#[test]
fn cada_limiar_compra_a_resolucao_que_promete() {
    // 0,10 bits por pixel é o piso para tela com texto. Abaixo disso a borda
    // da fonte vira bloco, e foi assim que «ruim com pouca gente» chegou.
    //
    // Em cima do limiar, e não acima: o limiar é o pior caso do degrau, e é
    // ele que tem de fechar a conta. Se fechar só com folga, o degrau está
    // prometendo o que não entrega na hora em que alguém de fato o alcança.
    const PISO_BPP: f64 = 0.10;

    for (teto, resolucao) in [
        (TETO_ESTIMADO_PARA_1080P_BPS, Resolucao::P1080),
        (TETO_ESTIMADO_PARA_720P_BPS, Resolucao::P720),
    ] {
        let cadencia = cadencia_para(teto, resolucao, Prioridade::Nitidez, Cadencia::Q30);
        let bpp = bits_por_pixel(teto, resolucao, cadencia.hz());
        assert!(
            bpp >= PISO_BPP,
            "{resolucao:?} no limiar de {teto} bps a {} quadros dá {bpp:.3} bpp, \
             abaixo do piso de {PISO_BPP} — é o limiar prometendo o que não paga",
            cadencia.hz(),
        );
    }
}

#[test]
fn o_teto_da_estimativa_alcanca_os_oito_megabits_que_o_produto_quer() {
    // O produto pede 8 Mbps de vídeo. O vídeo leva FRACAO_DO_CAMINHO — 60% —
    // do caminho, então o caminho tem de chegar a 13,3 Mbps para o vídeo
    // chegar a 8. Sem esta folga, «quero 8 Mbps» é uma escolha que o `min()`
    // das três pernas do §5.1 derruba em silêncio.
    let maior_video_bps =
        u64::from(crate::caminho::TETO_DA_ESTIMATIVA_BPS) * u64::from(FRACAO_DO_CAMINHO) / 100;
    assert!(
        maior_video_bps >= 8_000_000,
        "o teto da estimativa só chega a {maior_video_bps} bps de vídeo, e o produto quer 8 M"
    );
}

#[test]
fn oito_megabits_compram_1080p_a_trinta_com_folga() {
    // A ponta de cima da escada, escrita para não voltar a ser suposição: o
    // que o pedido de 8 Mbps de fato entrega na tela.
    let resolucao = resolucao_para(8_000_000, Prioridade::Nitidez);
    assert_eq!(resolucao, Resolucao::P1080);
    let cadencia = cadencia_para(8_000_000, resolucao, Prioridade::Nitidez, Cadencia::Q30);
    assert_eq!(cadencia, Cadencia::Q30);
    assert!(bits_por_pixel(8_000_000, resolucao, cadencia.hz()) >= 0.12);
}
```

- [ ] **Passo 2: rodar os testes e ver falhar**

Rodar: `cargo test -p seele-core tela::testes -- --nocapture`
Esperado: FALHA em `cada_limiar_compra_a_resolucao_que_promete` com algo como *«P1080 no limiar de 1500000 bps a 30 quadros dá 0.024 bpp»*, e FALHA em `o_teto_da_estimativa_alcanca_os_oito_megabits` (10 M × 60% = 6 M < 8 M).

- [ ] **Passo 3: recalibrar as três constantes**

Em `crates/seele-core/src/tela.rs`, trocar os valores e **reescrever a justificativa** — o comentário atual argumenta a partir da tabela do OpenH264 faminto e deixaria de valer:

```rust
pub const TETO_ESTIMADO_PARA_1080P_BPS: u32 = 6_200_000;
```
```rust
pub const TETO_ESTIMADO_PARA_720P_BPS: u32 = 2_800_000;
```

E `bits_por_quadro`, que passa a ser 0,10 bits por pixel em vez do gasto do codificador faminto:

```rust
pub const fn bits_por_quadro(resolucao: Resolucao) -> u32 {
    // **0,10 bits por pixel, e não o que o OpenH264 gastou.**
    //
    // A tabela anterior — 45 k a 1080p, 33 k a 720p, 30 k a 540p — era o gasto
    // por quadro **entregue** do codificador de referência num teto de
    // 1200 kbps. Naquele ponto ele estava jogando 16,2% dos quadros fora: é o
    // ponto de fome dele, não o ponto em que a imagem fica boa. Adotá-lo como
    // alvo fez o produto escolher sempre mais quadros e mais resolução do que
    // os bits pagavam, e é a aritmética inteira do relato «ruim com pouca
    // gente».
    //
    // 0,10 bpp é onde a borda de uma fonte para de virar bloco. Os três números
    // abaixo são essa constante vezes os pixels de cada degrau, arredondados
    // para cima — nada mais.
    match resolucao {
        // 2 073 600 px × 0,10
        Resolucao::P1080 => 207_000,
        //   921 600 px × 0,10
        Resolucao::P720 => 92_000,
        //   518 400 px × 0,10
        Resolucao::P540 => 52_000,
    }
}
```

Em `crates/seele-core/src/caminho.rs`:

```rust
/// O maior caminho que a sonda chega a afirmar, em bits por segundo.
///
/// **14 Mbps, e é a conta do produto e não uma folga.** O pedido é 8 Mbps de
/// vídeo — o que compra 1080p a 30 quadros com 0,13 bits por pixel, ou 720p a
/// 60 com 0,145. O vídeo leva [`crate::tela::FRACAO_DO_CAMINHO`], 60%, então
/// 8 Mbps de vídeo pedem 13,3 Mbps de caminho, e 14 é esse número com o
/// arredondamento para cima.
///
/// **O que este teto era, e por que mudou.** Ele valia 10 Mbps, justificados
/// como «quatro vezes os 2,5 Mbps de caminho que compram 1080p». O erro estava
/// no outro lado da conta e não aqui: aqueles 2,5 Mbps vinham de
/// `TETO_ESTIMADO_PARA_1080P_BPS` valer 1500 kbps, que é o ponto em que 1080p
/// passa a ser **comprável** — 0,024 bits por pixel — e não o ponto em que ele
/// fica bom. Com o limiar recalibrado para 6,2 Mbps, 10 Mbps de caminho dão
/// 6 Mbps de vídeo e mal alcançam o próprio degrau de cima.
///
/// Continua havendo um teto, e pela razão de sempre: acima de 1080p a 60 não
/// há degrau que a lista fechada do §5 ofereça, então uma estimativa maior só
/// produziria saltos que nada compra.
pub const TETO_DA_ESTIMATIVA_BPS: u32 = 14_000_000;
```

- [ ] **Passo 4: rodar os testes e ver passar**

Rodar: `cargo test -p seele-core -- --nocapture`
Esperado: PASSA. **Vários testes antigos vão ficar vermelhos**, e isso é o esperado, não um acidente: eles afirmam degraus nos limiares velhos. Para cada um, confira que o valor novo é o certo pela conta de bits por pixel **antes** de atualizar o número — um teste atualizado sem essa conferência é um teste que deixou de provar alguma coisa. Em particular, `a_sala_que_cresce_aperta_o_codificador_e_nao_a_voz` em `video.rs` muda de degrau em cada passo da escada.

- [ ] **Passo 5: olhar, e não só medir**

Rodar o app, compartilhar uma janela com texto pequeno — um editor de código serve — e conferir em três tetos: ~1,2 Mbps, ~3 Mbps e ~8 Mbps. O que se procura não é PSNR: é se o texto **se lê**. Se 540p nítido a 1,2 Mbps for ilegível no monitor do teste, o limiar de 720p está alto demais para este produto e o número certo está entre 2,8 M e o antigo 900 k — anote qual e por quê, porque isso é medida de produto e não de codec.

- [ ] **Passo 6: commit**

```bash
git add crates/seele-core/src/tela.rs crates/seele-core/src/caminho.rs crates/seele-core/src/video.rs
git commit -m "fix(tela): a régua passa a ser bits por pixel, e o teto alcança os 8 Mbps"
```

---

### Task 1: `ajustar_quadros` entra na costura `CodificaVideo`

Hoje `Codificador::ajustar_quadros` existe só no codificador de software, como método inerente, e o doc do trait diz por escrito: *«não está aqui porque ninguém o chama»*. A partir da Task 2 alguém chama, e os codificadores de hardware precisam saber responder. Mudar a cadência **não** custa quadro-chave e **não** reabre o fluxo — ao contrário da resolução, ela não vai no cabeçalho do §3.6.

**Arquivos:**
- Modificar: `crates/seele-video/src/codec.rs` (trait `CodificaVideo`, ~linha 380; `impl CodificaVideo for Codificador`, ~linha 420)
- Modificar: `crates/seele-video/src/codec/macos.rs` (`impl CodificaVideo for Codificador`, ~linha 585)
- Modificar: `crates/seele-video/src/codec/windows.rs` (`impl CodificaVideo for Codificador`, ~linha 640)
- Teste: `crates/seele-video/tests/ida_e_volta.rs` — e **não** o `mod testes` de `codec.rs`, que é só prova de compilação (`_e_send`) e não carrega o módulo do Cisco. `ida_e_volta.rs:61` já tem `fn biblioteca() -> Option<BibliotecaDeVideo>`, que é o guarda que faz o teste sair cedo quando o módulo não está na máquina.

**Interfaces:**
- Produz: `CodificaVideo::ajustar_quadros(&mut self, quadros_por_segundo: u32) -> Result<u32, ErroDeVideo>` — grampeia o pedido em `[PISO_DE_QUADROS, self.cadencia().hz()]`, aplica, e devolve **o valor que passou a valer**. A Task 2 depende desta assinatura.

- [ ] **Passo 1: escrever o teste que falha**

Em `crates/seele-video/tests/ida_e_volta.rs`, ao lado dos testes que já existem lá:

```rust
#[test]
fn a_costura_deixa_baixar_a_cadencia_sem_refazer_o_codificador() {
    let Some(biblioteca) = biblioteca() else {
        return;
    };
    let mut codificador = armar(
        &biblioteca,
        ConfigDoCodificador {
            resolucao: Resolucao::P540,
            cadencia: Cadencia::Q30,
            teto_bps: 600_000,
        },
    )
    .expect("armar o codificador");

    // O pedido cabe na faixa: vale exatamente o que se pediu.
    assert_eq!(codificador.ajustar_quadros(15), Ok(15));
    assert_eq!(codificador.quadros_por_segundo(), 15);

    // A cadência escolhida continua sendo teto: pedir mais que ela não sobe.
    assert_eq!(codificador.ajustar_quadros(60), Ok(30));
    assert_eq!(codificador.quadros_por_segundo(), 30);

    // E o piso do §2 não é atravessável.
    assert_eq!(codificador.ajustar_quadros(1), Ok(PISO_DE_QUADROS));
    assert_eq!(codificador.quadros_por_segundo(), PISO_DE_QUADROS);

    // A resolução não se mexeu: mudar cadência não é refazer nada.
    assert_eq!(codificador.resolucao(), Resolucao::P540);
}
```

- [ ] **Passo 2: rodar o teste e ver falhar**

Rodar: `cargo test -p seele-video a_costura_deixa_baixar_a_cadencia -- --nocapture`
Esperado: FALHA de compilação — `no method named 'ajustar_quadros' found for struct 'Box<dyn CodificaVideo>'`.

- [ ] **Passo 3: pôr o método no trait**

Em `crates/seele-video/src/codec.rs`, dentro de `pub trait CodificaVideo`, logo depois de `ajustar_teto`:

```rust
    /// Muda quantos quadros por segundo ele mira, dentro da faixa automática.
    ///
    /// **Não custa quadro-chave e não reabre o fluxo**, e é essa a diferença
    /// para a resolução: a resolução mora no cabeçalho de abertura do §3.6 e
    /// trocá-la é recomeçar três coisas; a cadência não vai no fio, então
    /// baixá-la é uma propriedade de sessão e nada mais.
    ///
    /// O pedido é **grampeado** entre [`PISO_DE_QUADROS`] e a cadência com que
    /// este codificador foi armado — que é teto e nunca piso (§5) —, e o que
    /// volta é o valor que passou a valer, não o que se pediu.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::CodecRecusou`] quando o codificador não aceita o número.
    fn ajustar_quadros(&mut self, quadros_por_segundo: u32) -> Result<u32, ErroDeVideo>;
```

E no `impl CodificaVideo for Codificador` do mesmo arquivo, encaminhando para o método inerente que já existe:

```rust
    fn ajustar_quadros(&mut self, quadros_por_segundo: u32) -> Result<u32, ErroDeVideo> {
        Self::ajustar_quadros(self, quadros_por_segundo)
    }
```

- [ ] **Passo 4: implementar no VideoToolbox**

Em `crates/seele-video/src/codec/macos.rs`, dentro de `impl CodificaVideo for Codificador`:

```rust
    fn ajustar_quadros(&mut self, quadros_por_segundo: u32) -> Result<u32, ErroDeVideo> {
        let valendo = quadros_por_segundo.clamp(super::PISO_DE_QUADROS, self.cadencia.hz());
        if valendo == self.quadros_por_segundo {
            return Ok(valendo);
        }
        // `ExpectedFrameRate` é o que o controle de taxa usa para repartir o
        // orçamento entre os quadros que espera receber. Sem esta linha o
        // VideoToolbox continuaria orçando para 30 enquanto recebe 15, e
        // gastaria metade do teto — o avesso do que esta mudança quer.
        //
        // SAFETY: a chave é estática da própria biblioteca.
        unsafe {
            ajustar(
                &self.sessao.0,
                kVTCompressionPropertyKey_ExpectedFrameRate,
                CFNumber::new_i32(i32::try_from(valendo).unwrap_or(30)).as_ref(),
                "mudar a cadência esperada",
            )?;
        }
        self.quadros_por_segundo = valendo;
        Ok(valendo)
    }
```

O carimbo de tempo de cada quadro já sai de `contador / quadros_por_segundo` (ver o comentário em `macos.rs:863`), então ele acompanha sozinho.

- [ ] **Passo 5: implementar no Media Foundation**

Em `crates/seele-video/src/codec/windows.rs`, dentro de `impl CodificaVideo for Codificador`:

```rust
    fn ajustar_quadros(&mut self, quadros_por_segundo: u32) -> Result<u32, ErroDeVideo> {
        let valendo = quadros_por_segundo.clamp(super::PISO_DE_QUADROS, self.cadencia.hz());
        // **Só o número, e não o tipo de mídia.** Trocar `MF_MT_FRAME_RATE`
        // com o fluxo aberto exige `MFT_MESSAGE_COMMAND_FLUSH` mais
        // `SetOutputType`, e as duas coisas juntas custam um IDR — que é
        // exatamente o preço que esta porta existe para não pagar.
        //
        // O que faz o MFT gastar mais bits em cada quadro é receber menos
        // quadros com a duração certa: `self.quadros_por_segundo` já é quem
        // calcula `SetSampleTime` e `SetSampleDuration` (ver `amostra`), e o
        // CBR persegue a média ao longo de um segundo qualquer que seja a
        // contagem.
        self.quadros_por_segundo = valendo;
        Ok(valendo)
    }
```

- [ ] **Passo 6: rodar o teste e ver passar**

Rodar: `cargo test -p seele-video a_costura_deixa_baixar_a_cadencia -- --nocapture`
Esperado: PASSA (ou sai cedo, sem falhar, se o módulo do Cisco não estiver na máquina).

Rodar também a suíte inteira do crate, que é onde a costura é exercida: `cargo test -p seele-video`
Esperado: tudo verde.

- [ ] **Passo 7: commit**

```bash
git add crates/seele-video/src/codec.rs crates/seele-video/src/codec/macos.rs crates/seele-video/src/codec/windows.rs
git commit -m "feat(codec): a costura passa a deixar baixar a cadência sem refazer nada"
```

---

### Task 2: a cadência cede também quando a sala cresce

Este é o defeito. `config_para` chama `cadencia_para` e resolve o orçamento **ao armar**; `Compartilhamento::ajustar` — que é por onde o teto de fato anda, porque é onde alguém entra ou sai da sala — recalcula só a resolução. Numa casa que hospeda com 6 Mbps, seis espectadores dão 600 kbps: a resolução desce para 540p e a cadência **fica em 30**, ou seja 20 kbit por quadro, 0,038 bits por pixel. É o bloco que aparece com sala cheia.

**Arquivos:**
- Modificar: `crates/seele-core/src/video.rs` — `enum Ajuste` (linha 804), `Compartilhamento::ajustar` (a partir da linha 996)
- Teste: `crates/seele-core/src/video.rs`, módulo de testes no fim do arquivo

**Interfaces:**
- Consome: `CodificaVideo::ajustar_quadros` da Task 1; `seele_core::tela::cadencia_para(teto_bps: u32, resolucao: Resolucao, prioridade: Prioridade, escolha: Cadencia) -> Cadencia`, que já existe.
- Produz: variante `Ajuste::CadenciaNova { quadros_por_segundo: u32 }`. A Task 3 e o `bomba.rs` a consomem.

- [ ] **Passo 1: escrever o teste que falha**

Em `crates/seele-core/src/video.rs`, no módulo de testes, ao lado de `a_sala_que_cresce_aperta_o_codificador_e_nao_a_voz`:

```rust
#[test]
fn a_sala_que_cresce_faz_o_quadro_ceder_e_nao_so_a_resolucao() {
    // O §2: «a resolução segura, o quadro cede». `config_para` já cumpre isso
    // ao armar; este teste é a mesma regra no caminho onde o teto de fato anda,
    // que é alguém entrando na sala (§5.1).
    let Some(biblioteca) = biblioteca() else {
        return;
    };

    // Uma casa que hospeda com 6 Mbps de subida, sozinha na sala: 3,6 Mbps.
    let sozinho = TetoDeVideo::com_caminho(6_000_000)
        .com_caminho_de_quem_hospeda(6_000_000)
        .com_espectadores(1);
    let mut compartilhamento = Compartilhamento::abrir(
        biblioteca,
        &sozinho,
        SignalBand::Nominal,
        Resolucao::P1080,
        Cadencia::Q30,
        Prioridade::Nitidez,
    )
    .expect("armar o codificador com 3,6 Mbps de teto");
    assert_eq!(compartilhamento.quadros_por_segundo(), 30);

    // Seis pessoas assistindo: 3,6 Mbps ÷ 6 = 600 kbps. Abaixo de
    // TETO_ESTIMADO_PARA_720P_BPS (900k), então o degrau é 540p; e
    // 600_000 / bits_por_quadro(540p) = 20, que compra Q15 e não Q30.
    let a_seis = sozinho.com_espectadores(6);
    let ajuste = compartilhamento
        .ajustar(&a_seis, SignalBand::Nominal)
        .expect("baixar a banda do codificador");

    // A resolução continua sendo a notícia maior — ela reabre o fluxo.
    assert_eq!(
        ajuste,
        Ajuste::ResolucaoPedida {
            de: Resolucao::P1080,
            para: Resolucao::P540,
            teto_bps: 600_000,
        }
    );

    // **E o quadro cedeu junto, sem esperar a reabertura.** Baixar a cadência
    // não custa quadro-chave, então não há razão para ela esperar o degrau.
    assert_eq!(compartilhamento.quadros_por_segundo(), 15);
}

#[test]
fn a_cadencia_volta_quando_a_sala_esvazia() {
    // O contrário do teste acima, e ele existe porque um ajuste que só desce é
    // um catraca: quem saísse da sala deixaria todo mundo a 15 quadros para
    // sempre. A escolha da pessoa é teto (§5) e continua sendo alcançável.
    let Some(biblioteca) = biblioteca() else {
        return;
    };
    let sozinho = TetoDeVideo::com_caminho(6_000_000)
        .com_caminho_de_quem_hospeda(6_000_000)
        .com_espectadores(1);
    let mut compartilhamento = Compartilhamento::abrir(
        biblioteca,
        &sozinho,
        SignalBand::Nominal,
        Resolucao::P540,
        Cadencia::Q30,
        Prioridade::Nitidez,
    )
    .expect("armar o codificador");

    let _ = compartilhamento
        .ajustar(&sozinho.com_espectadores(6), SignalBand::Nominal)
        .expect("apertar");
    assert_eq!(compartilhamento.quadros_por_segundo(), 15);

    let de_volta = compartilhamento
        .ajustar(&sozinho.com_espectadores(1), SignalBand::Nominal)
        .expect("afrouxar");
    assert_eq!(compartilhamento.quadros_por_segundo(), 30);
    assert!(
        matches!(de_volta, Ajuste::TetoNovo { .. } | Ajuste::CadenciaNova { .. }),
        "a volta tem de ser notícia, e veio {de_volta:?}"
    );
}
```

- [ ] **Passo 2: rodar os testes e ver falhar**

Rodar: `cargo test -p seele-core a_sala_que_cresce_faz_o_quadro_ceder -- --nocapture`
Esperado: FALHA de compilação em `Ajuste::CadenciaNova` (variante inexistente). Depois de o Passo 3 criá-la, a falha esperada vira `assertion failed: left: 30, right: 15`.

- [ ] **Passo 3: acrescentar a variante ao `Ajuste`**

Em `crates/seele-core/src/video.rs`, dentro de `pub enum Ajuste`, entre `TetoNovo` e `ResolucaoPedida`:

```rust
    /// A cadência mudou, e a resolução não.
    ///
    /// Variante própria e não um campo em [`Self::TetoNovo`] porque as duas
    /// respondem perguntas diferentes na interface do §5 — «o que está saindo»
    /// tem dois números, e o que muda é quase sempre um só. Baixar a cadência
    /// **não** reabre o fluxo: ao contrário da resolução, ela não vai no
    /// cabeçalho do §3.6, então quem recebe isto não tem nada a refazer.
    CadenciaNova {
        /// Quantos quadros por segundo passaram a valer.
        quadros_por_segundo: u32,
    },
```

- [ ] **Passo 4: guardar a escolha de cadência e aplicá-la no `ajustar`**

Em `crates/seele-core/src/video.rs`, acrescentar o campo à struct `Compartilhamento` (que hoje tem `biblioteca`, `codificador`, `escolha_de_resolucao`, `prioridade`, `teto`):

```rust
    /// A cadência que a pessoa escolheu, que é **teto e nunca piso** (§5).
    ///
    /// Guardada à parte do codificador de propósito: o codificador carrega a
    /// que **está valendo**, já reduzida pelo orçamento, e ler a escolha dele
    /// faria cada aperto virar o novo máximo — uma catraca que nunca devolve os
    /// 30 quadros depois que a sala esvazia.
    escolha_de_cadencia: Cadencia,
```

Em `Compartilhamento::abrir`, no construtor do `Self`, acrescentar `escolha_de_cadencia: cadencia,` (o parâmetro já chega na função).

Em `Compartilhamento::ajustar`, **depois** do bloco `if mudou_a_banda { ... }` e **antes** do cálculo de `do_teto`, inserir:

```rust
        // **A outra metade do §2, no caminho onde o teto de fato anda.**
        //
        // `config_para` já resolve isto ao armar, e era só ali: a sala que
        // cresce chega por aqui, não por lá. Sem estas linhas, seis
        // espectadores numa casa de 6 Mbps derrubavam a resolução para 540p e
        // deixavam a cadência em 30 — 20 kbit por quadro, que é o bloco que o
        // relato chama de «pixelado».
        //
        // Contra a resolução, e não contra a que está armada: os bits que um
        // quadro precisa dependem de quantos pixels ele tem, e o degrau que vai
        // valer é o que sai logo abaixo. Por isso a conta usa `degrau_novo`.
        let degrau_novo = menor_resolucao(
            crate::tela::resolucao_para(bps, self.prioridade),
            self.escolha_de_resolucao,
        );
        let cadencia_alvo = crate::tela::cadencia_para(
            bps,
            degrau_novo,
            self.prioridade,
            self.escolha_de_cadencia,
        );
        let antes = self.codificador.quadros_por_segundo();
        let agora = self.codificador.ajustar_quadros(cadencia_alvo.hz())?;
        let mudou_a_cadencia = agora != antes;
        if mudou_a_cadencia {
            tracing::info!(
                de = antes,
                para = agora,
                teto_bps = bps,
                "a cadência da tela cedeu ao orçamento"
            );
        }
```

Trocar o cálculo de `do_teto`/`degrau` logo abaixo por um reaproveitamento de `degrau_novo`, e acrescentar o braço novo na escada de retorno:

```rust
        if degrau_novo != self.codificador.resolucao() {
            return Ok(Ajuste::ResolucaoPedida {
                de: self.codificador.resolucao(),
                para: degrau_novo,
                teto_bps: bps,
            });
        }
        if mudou_a_cadencia {
            return Ok(Ajuste::CadenciaNova {
                quadros_por_segundo: agora,
            });
        }
        if mudou_a_banda {
            return Ok(Ajuste::TetoNovo { teto_bps: bps });
        }
        Ok(Ajuste::Igual)
```

A ordem é deliberada: a resolução ganha porque é a única que obriga a reabrir o fluxo, e quem recebe `ResolucaoPedida` vai reler a cadência de qualquer forma ao refazer.

- [ ] **Passo 5: rodar os testes e ver passar**

Rodar: `cargo test -p seele-core video::testes -- --nocapture`
Esperado: PASSA, incluindo `a_sala_que_cresce_aperta_o_codificador_e_nao_a_voz`, que não deve ter mudado de resultado — ele afirma resolução e teto, não cadência.

- [ ] **Passo 6: tratar a variante nova na bomba**

`crates/seele-core/src/bomba.rs`, em `teto_andou`, o `match ajuste` vai reclamar de padrão não coberto. Acrescentar:

```rust
            Ok(Ajuste::CadenciaNova {
                quadros_por_segundo,
            }) => {
                // Nada a rearmar: o laço lê `quadros_por_segundo()` a cada
                // `dormir()`, então o intervalo entre tiques já acompanha. O que
                // falta é a interface saber, porque o §5 obriga a mostrar o que
                // está saindo ao lado do que foi pedido.
                self.emitir(EventoDaBomba::Cadencia {
                    quadros_por_segundo,
                });
                Ok(())
            }
```

E acrescentar a variante correspondente a `EventoDaBomba`, ao lado de `Teto { teto_bps }`:

```rust
    /// A cadência que está saindo mudou (§5).
    Cadencia {
        /// Quantos quadros por segundo passaram a valer.
        quadros_por_segundo: u32,
    },
```

- [ ] **Passo 7: rodar a suíte inteira**

Rodar: `cargo test -p seele-core -p seele-video`
Esperado: tudo verde. Se `seele-ffi` ou `seele-app` deixarem de compilar por causa do `EventoDaBomba` novo, encaminhe o evento à interface do mesmo jeito que `EventoDaBomba::Teto` já é encaminhado — é o par que o §5 pede.

- [ ] **Passo 8: commit**

```bash
git add crates/seele-core/src/video.rs crates/seele-core/src/bomba.rs
git commit -m "fix(tela): o quadro cede também quando a sala cresce, e volta quando ela esvazia"
```

---

### Task 3: refazer o codificador não perde a escolha da pessoa

`refazer_com` monta o `ConfigDoCodificador` com `cadencia: self.codificador.cadencia()` — a cadência **do codificador**, que já pode estar reduzida. Como esse campo é o **máximo**, cada reabertura de fluxo transforma a redução de ontem no teto de hoje: quem escolheu 60 e passou por um aperto nunca mais volta a 60. E ao mudar de degrau os bits por quadro mudam (45 k a 1080p contra 30 k a 540p), então a cadência precisa ser recalculada para a resolução nova, não carregada da velha.

**Arquivos:**
- Modificar: `crates/seele-core/src/video.rs` — `Compartilhamento::refazer_com` (a partir da linha 1062)
- Teste: `crates/seele-core/src/video.rs`, módulo de testes

**Interfaces:**
- Consome: `escolha_de_cadencia` da Task 2; `crate::tela::cadencia_para`.

- [ ] **Passo 1: escrever o teste que falha**

```rust
#[test]
fn refazer_o_degrau_nao_transforma_o_aperto_de_ontem_em_teto() {
    // O campo `cadencia` de `ConfigDoCodificador` é o **máximo** (§5). Passar
    // para ele a cadência já reduzida faz cada reabertura de fluxo baixar o
    // teto mais um degrau, e a escolha da pessoa some sem ninguém a revogar.
    let Some(biblioteca) = biblioteca() else {
        return;
    };
    let sozinho = TetoDeVideo::com_caminho(6_000_000)
        .com_caminho_de_quem_hospeda(6_000_000)
        .com_espectadores(1);
    let mut compartilhamento = Compartilhamento::abrir(
        biblioteca,
        &sozinho,
        SignalBand::Nominal,
        Resolucao::P1080,
        Cadencia::Q30,
        Prioridade::Nitidez,
    )
    .expect("armar o codificador");

    // Aperta: 600 kbps, 540p, e a cadência cede para 15.
    let _ = compartilhamento
        .ajustar(&sozinho.com_espectadores(6), SignalBand::Nominal)
        .expect("apertar");
    compartilhamento
        .refazer_com(Resolucao::P540)
        .expect("refazer em 540p");
    assert_eq!(compartilhamento.quadros_por_segundo(), 15);

    // A sala esvazia: 3,6 Mbps. A escolha de 30 continua alcançável, e é ela
    // que tem de voltar — não os 15 de que o codificador foi refeito.
    let _ = compartilhamento
        .ajustar(&sozinho.com_espectadores(1), SignalBand::Nominal)
        .expect("afrouxar");
    assert_eq!(
        compartilhamento.quadros_por_segundo(),
        30,
        "a escolha da pessoa é teto e não pode ser comida por um aperto passado"
    );
}
```

- [ ] **Passo 2: rodar o teste e ver falhar**

Rodar: `cargo test -p seele-core refazer_o_degrau_nao_transforma -- --nocapture`
Esperado: FALHA — `left: 15, right: 30`, porque `refazer_com` fixou o teto em 15.

- [ ] **Passo 3: corrigir `refazer_com`**

Trocar o corpo de `refazer_com` por:

```rust
    pub fn refazer_com(&mut self, resolucao: Resolucao) -> Result<(), ErroDeCompartilhamento> {
        let config = ConfigDoCodificador {
            resolucao,
            // **A escolha da pessoa, e não a cadência que está valendo.**
            //
            // Este campo é o **máximo** (§5), e o codificador carrega o valor
            // já reduzido pelo orçamento. Passar o reduzido faria cada
            // reabertura de fluxo virar o teto novo — quem escolheu 60 e passou
            // por um aperto nunca mais voltaria a 60, sem ninguém ter revogado
            // a escolha.
            cadencia: self.escolha_de_cadencia,
            teto_bps: self.teto.bps(),
        };
        self.codificador = armar(&self.biblioteca, config)?;
        // E o orçamento decide de novo, agora contra o degrau novo: os bits que
        // um quadro precisa dependem de quantos pixels ele tem — 45 k a 1080p
        // contra 30 k a 540p —, então a cadência da resolução velha não serve.
        let alvo = crate::tela::cadencia_para(
            self.teto.bps(),
            resolucao,
            self.prioridade,
            self.escolha_de_cadencia,
        );
        self.codificador.ajustar_quadros(alvo.hz())?;
        Ok(())
    }
```

- [ ] **Passo 4: rodar os testes e ver passar**

Rodar: `cargo test -p seele-core video::testes -- --nocapture`
Esperado: PASSA, os três testes novos e todos os antigos.

- [ ] **Passo 5: commit**

```bash
git add crates/seele-core/src/video.rs
git commit -m "fix(tela): refazer o degrau deixa de comer a cadência que a pessoa escolheu"
```

---

### Task 4: ~~o Media Foundation passa a declarar o perfil~~ — RETIRADA, a premissa era falsa

**Medido em 2026-09-04, numa máquina Windows de verdade, e o resultado foi o contrário do previsto.**

A tarefa dizia que `fn tipo` monta o tipo de saída sem `MF_MT_MPEG2_PROFILE` e que, sem perfil declarado, o codificador H.264 da Microsoft entrega **Baseline** — CAVLC, atrás do caminho de software deste mesmo crate, que usa CABAC desde 2026-08-31. Era o suspeito nomeado para o relato «está mais pixelado que antes, no Windows».

O teste que a tarefa mandava escrever foi escrito e rodado. Ele lê o `profile_idc` do SPS, que é o fio e não a API de nenhum sistema:

```
PERFIL NO FIO: profile_idc=77 (Main), quadro-chave de 173 bytes
```

**77 é Main.** O MFT já escolhe Main sem que ninguém peça, e o `Decodificador` do OpenH264 abre o fluxo. A ausência de `MF_MT_MPEG2_PROFILE` não estava custando nada nesta máquina.

O que **sobrou de valor**: o teste `o_codificador_armado_nao_sai_em_baseline` fica em `crates/seele-video/tests/ida_e_volta.rs`. Ele não conserta nada — prende. Um driver de outro fabricante, ou uma versão de Windows que escolha Baseline, passa a ser reprovado em vez de descoberto por relato de campo. É a única forma honesta de uma suposição virar garantia: escrevê-la como teste e deixá-la rodar em toda máquina.

**Um achado de lado, e não é cosmético:** `seele_proto::screen::ScreenCodec` só tem a variante `H264Baseline`, e é o byte 0 que viaja no cabeçalho de abertura do §3.6. O que sai do Windows é **Main**. O cabeçalho declara um perfil que o fluxo não tem. Hoje ninguém se machuca — quem assiste decodifica por WebCodecs, que lê o perfil do próprio SPS —, mas o doc daquele enum diz que ele existe para que *«o receptor recuse um codec que não sabe decodificar»*, e um campo que mente não serve para isso. Merece pendência própria.

**Lição para quem escrever a próxima tarefa deste plano:** «o padrão da API é X» é hipótese, não fato, e o custo de conferir era um teste de trinta linhas.

### Task 5: o VideoToolbox passa a declarar o perfil em vez de herdá-lo do driver

`Codificador::novo` em `codec/macos.rs` declara tempo real, ausência de reordenação, cadência esperada, intervalo de quadro-chave e banda — e **não** declara `ProfileLevel`. O que sai fica com o que o driver escolher, que varia por máquina e por versão do sistema. Uma propriedade que decide a qualidade do produto não pode ser um padrão de terceiro.

**Arquivos:**
- Modificar: `crates/seele-video/src/codec/macos.rs` — bloco `use` (linha ~44) e `Codificador::novo` (linha ~410)
- Teste: `crates/seele-video/tests/ida_e_volta.rs` (o mesmo `o_codificador_armado_nao_sai_em_baseline` da Task 4 cobre este lado)

**Interfaces:**
- Consome: `profile_idc` e `o_codificador_armado_nao_sai_em_baseline` da Task 4.

- [ ] **Passo 1: rodar o teste da Task 4 no macOS e registrar o que sai**

Rodar: `cargo test -p seele-video o_codificador_armado_nao_sai_em_baseline -- --nocapture`
Anote o `profile_idc` que aparece na falha, se houver. Este passo é medição: pode ser que o driver desta máquina já escolha 100, e mesmo assim a linha entra — o defeito é depender da escolha dele, não o valor de hoje.

- [ ] **Passo 2: declarar o perfil**

Acrescentar ao bloco `use` de `objc2_video_toolbox` em `crates/seele-video/src/codec/macos.rs`:

```rust
    kVTCompressionPropertyKey_ProfileLevel, kVTProfileLevel_H264_Main_AutoLevel,
```

E em `Codificador::novo`, dentro do bloco `unsafe { ... }` que já declara `RealTime` e companhia, **antes** de `RealTime` (o perfil governa o resto):

```rust
            // **O perfil, declarado e não herdado.**
            //
            // Sem esta linha o que sai é o que o driver desta máquina escolher,
            // e isso muda por geração e por versão do sistema — uma propriedade
            // que decide a qualidade do produto não pode ser padrão de
            // terceiro. O irmão do Windows tem o mesmo problema e a mesma
            // correção, em `MF_MT_MPEG2_PROFILE`.
            //
            // Main e não High: `tests/ida_e_volta.rs` decodifica pelo OpenH264,
            // e o binding diz por escrito que a transformada 8×8 exigida por
            // High não está implementada. Main é CABAC sem 8×8, que é
            // exatamente o que o caminho de software deste crate já emite.
            ajustar(
                &sessao.0,
                kVTCompressionPropertyKey_ProfileLevel,
                kVTProfileLevel_H264_Main_AutoLevel,
                "declarar o perfil H.264",
            )?;
```

Se `objc2-video-toolbox` 0.3.2 não expuser `kVTProfileLevel_H264_Main_AutoLevel`, o valor é a `CFString` `"H264_Main_AutoLevel"`; construa-a com `objc2_core_foundation::CFString::from_str("H264_Main_AutoLevel")` e passe `.as_ref()`. Não invente outra grafia: a string é a chave.

- [ ] **Passo 3: rodar os testes e ver passar**

Rodar: `cargo test -p seele-video -- --nocapture`
Esperado: PASSA. `o_codificador_armado_nao_sai_em_baseline` lê 77, e `ida_e_volta` continua verde.

Se a sessão recusar a propriedade com `kVTPropertyNotSupportedErr`, o `?` derruba `Codificador::novo` e `armar` cai para o OpenH264 — o que é seguro mas perde o hardware. Nesse caso, troque o `?` por um `let _ =` com `tracing::info!` do erro, seguindo o formato de `botao` no Windows: *nenhum destes é obrigatório e nenhum derruba nada ao falhar.*

- [ ] **Passo 4: commit**

```bash
git add crates/seele-video/src/codec/macos.rs
git commit -m "fix(codec): o VideoToolbox passa a declarar o perfil em vez de herdá-lo do driver"
```

---

### Task 6: o arnês de qualidade passa a comparar perfis e cadências

`tests/qualidade-do-codec.rs` já mede PSNR por codificador e imprime o número sem reprovar por ele. O que ele não faz é variar o eixo que este plano mexeu — cadência —, e é por isso que a discussão sobre «quantos %» só tem estimativa atrás. Esta tarefa transforma o arnês na resposta.

**Arquivos:**
- Modificar: `crates/seele-video/tests/qualidade-do-codec.rs`

**Interfaces:**
- Consome: `CodificaVideo::ajustar_quadros` da Task 1.

- [ ] **Passo 1: fazer `medir` parar de supor 30 quadros**

`fn medir` em `crates/seele-video/tests/qualidade-do-codec.rs` calcula os segundos da medida com `quadros.len() as f64 / 30.0` — o 30 escrito à mão. Enquanto a cadência era sempre 30 isso estava certo; a partir da Task 1 não está mais, e uma medida a 8 quadros sairia com o bitrate quatro vezes maior do que foi.

Trocar, dentro de `fn medir`, a linha:

```rust
    let segundos = quadros.len() as f64 / 30.0;
```

por:

```rust
    // A cadência do codificador, e não um 30 escrito à mão: a partir do momento
    // em que `ajustar_quadros` existe, medir 120 quadros a 8 por segundo e
    // dividir por 30 diz que saíram quatro vezes mais bits do que saíram.
    let segundos = quadros.len() as f64 / f64::from(codificador.quadros_por_segundo().max(1));
```

- [ ] **Passo 2: escrever a medida**

Acrescentar ao fim de `crates/seele-video/tests/qualidade-do-codec.rs`, reaproveitando `medir`, `Medida`, `quadro_de_tela`, `QUADROS` e `pastas()`, que o arquivo já tem:

```rust
/// Quanta qualidade cada cadência compra pelo mesmo teto.
///
/// É a tabela que faltava para o §2 deixar de ser argumento e virar número: «a
/// resolução segura, o quadro cede» diz **que** o quadro cede, e não quanto se
/// ganha com isso. Como o resto deste arquivo, não reprova por qualidade —
/// imprime, e quem estiver perguntando lê.
#[test]
fn quanto_a_cadencia_compra_pelo_mesmo_teto() {
    let Ok(biblioteca) = BibliotecaDeVideo::procurar_e_carregar(&pastas()) else {
        eprintln!("PULADO: sem o módulo do Cisco não há com o que comparar.");
        return;
    };

    eprintln!("\n  resolução | teto  | quadros |   PSNR   |  Mbps  | saíram");
    for resolucao in [Resolucao::P540, Resolucao::P720] {
        let quadros: Vec<QuadroI420> = (0..QUADROS)
            .map(|passo| quadro_de_tela(resolucao, passo))
            .collect();
        for teto_bps in [600_000_u32, 1_200_000] {
            for pedido in [8_u32, 15, 30] {
                let mut codificador = armar(
                    &biblioteca,
                    ConfigDoCodificador {
                        resolucao,
                        cadencia: Cadencia::Q30,
                        teto_bps,
                    },
                )
                .expect("armar o codificador");
                let valendo = codificador
                    .ajustar_quadros(pedido)
                    .expect("ajustar a cadência");
                let medida = medir(codificador.as_mut(), &quadros, &biblioteca);
                eprintln!(
                    "  {:>9?} | {:>3} k | {valendo:>7} | {:>5.2} dB | {:>6.2} | {:>3} de {QUADROS}",
                    resolucao,
                    teto_bps / 1000,
                    medida.psnr,
                    medida.mbps,
                    medida.sairam,
                );
            }
        }
    }
}
```

- [ ] **Passo 3: rodar e guardar a linha de base**

Rodar: `cargo test -p seele-video quanto_a_cadencia_compra -- --nocapture`
Esperado: PASSA, e imprime a tabela. Guarde a saída — é contra ela que a próxima mudança se compara, e é ela que responde «melhorou quantos por cento» com número em vez de aritmética de guardanapo.

- [ ] **Passo 4: commit**

```bash
git add crates/seele-video/tests/qualidade-do-codec.rs
git commit -m "test(codec): o arnês passa a medir o que a cadência compra pelo mesmo teto"
```

---

## Verificação final

- [ ] `cargo test --workspace` verde no macOS
- [ ] `cargo test --workspace` verde no Windows
- [ ] `cargo clippy --workspace --all-targets` sem avisos novos
- [ ] `cargo xtask check-deps` verde (nenhuma tarefa acrescenta aresta de dependência)
- [ ] A tabela da Task 6 rodada nos dois sistemas, com a saída anexada ao PR

## O que este plano não faz

- **Não mexe no `÷N` do §5.1.** Numa casa que hospeda com 3 Mbps, nove espectadores continuam batendo em `PISO_DE_BANDA_BPS` e a transmissão continua parando. Isso é topologia, não codec, e a saída que existe hoje é hospedar num `seeled` com subida de servidor.
- **Não toca no lado de quem assiste.** Não há buffer de jitter para a tela, o carimbo de apresentação é o instante de chegada e cada quadro atravessa a ponte em base64. Isso é o outro pedido — «fluida do início ao fim» — e é um plano próprio, porque mexe em `seele-proto` (carimbo no cabeçalho de quadro), no `seele-ffi` e no JS, e não em codec nenhum.
- **Não sobe para High profile.** Fica dependendo da medida da Task 6 e de uma decisão sobre o `Decodificador` do OpenH264 nos testes.
- **Não sobe `MAX_QUADRO_LEN` nem `FATIAS_DO_QUADRO_CHAVE`.** A 8 Mbps o quadro-chave de 1080p30 fica em ~133 KB, bem dentro dos 512 KiB, e só a 8 quadros por segundo ele chega perto da borda (~500 KB). São constantes de **formato**, gêmeas em `seele-core` e `seele-server`, e mexer nelas é mudança de protocolo entre versões — merece tarefa própria, com as duas metades no mesmo commit.

---

### Task 7: a escada passa a lembrar onde parou

**O primeiro segundo é o pior que este produto tem, e é o único que todo mundo vê.**

`CAMINHO_DA_PROVA_BPS = 2_000_000`: toda transmissão parte supondo uma subida doméstica de 2 Mbps. Inclusive entre duas máquinas no mesmo switch que mediram 12,48 Mbps entre si vinte minutos antes. A medida morre quando a transmissão acaba, e a próxima redescobre do zero.

Medido em campo nesta sessão, LAN, duas máquinas:

```
02:25:31   2.400.000 →  4.524.054   → P720
02:25:40   4.524.054 →  9.048.108   → P1080
02:25:43   9.048.108 → 12.480.000   ← topo
```

**Doze segundos de imagem ruim para reaprender um fato que era verdade desde o primeiro milissegundo.** E o código já sabia: `IDA_E_VOLTA_CURTA` de 5 ms identificou o cano curto e trocou o passo de 125% para 200% — ou seja, ele reconheceu a LAN e mesmo assim começou supondo 2 Mbps.

O que **não** muda: a escolha da pessoa continua sendo teto e nunca piso (§5), e o teto continua cedendo quando o caminho aperta. Lembrar de onde começar não fere a regra do §3.2 — a primeira janela que doer corrige, pelo mecanismo que já existe.

**Arquivos:**
- Modificar: `crates/seele-core/src/conhecidos.rs` — `struct Conhecido`, e a leitura e escrita da linha
- Modificar: `crates/seele-core/src/caminho.rs` — `Sonda::nova` ganha irmã que parte de uma medida
- Teste: `crates/seele-core/src/conhecidos.rs` e `crates/seele-core/src/caminho.rs`, nos módulos de teste que já existem

**Interfaces:**
- Produz: `Conhecido::caminho_bps: Option<u32>` e `Sonda::partindo_de(bps: u32) -> Sonda`.

- [ ] **Passo 1: escrever o teste que falha, no `conhecidos`**

```rust
#[test]
fn a_linha_carrega_o_ultimo_caminho_medido() {
    // O quinto campo, e ele é de conveniência como os outros três: um arquivo
    // apagado custa doze segundos de imagem ruim uma vez, e nada mais. É por
    // isso que ele mora aqui e não junto dos pins — ver o cabeçalho.
    let escrito = "192.168.0.7:8383\tmarcela\t1\t1738000000\t12480000";
    let lido = Conhecido::da_linha(escrito).expect("uma linha de cinco campos");
    assert_eq!(lido.caminho_bps, Some(12_480_000));
    assert_eq!(lido.para_linha(), escrito);

    // **Quatro campos continua sendo uma linha válida**, e isto não é gentileza:
    // é o arquivo que já está no disco de quem atualizar. Sem esta linha, a
    // primeira execução da versão nova esquece todos os servidores conhecidos.
    let velha = "192.168.0.7:8383\tmarcela\t1\t1738000000";
    let lido = Conhecido::da_linha(velha).expect("uma linha de quatro campos");
    assert_eq!(lido.caminho_bps, None);
}
```

- [ ] **Passo 2: rodar e ver falhar**

Rodar: `cargo test -p seele-core conhecidos::`
Esperado: FALHA de compilação — `no field 'caminho_bps' on type 'Conhecido'`.

- [ ] **Passo 3: acrescentar o campo**

Em `struct Conhecido`:

```rust
    /// O último caminho medido para este servidor, em bits por segundo.
    ///
    /// `None` para uma linha escrita por uma versão anterior, e para um
    /// servidor onde ninguém compartilhou tela ainda. Quem lê isto é
    /// [`crate::caminho::Sonda::partindo_de`], e o que ele evita são os doze
    /// segundos que a escada gasta para reaprender uma LAN.
    ///
    /// **Conveniência, e não verdade.** Um número velho não vincula nada: a
    /// sonda continua medindo, e a primeira janela que doer o substitui. É por
    /// isso que ele pode morar num arquivo que se apaga sem consequência.
    pub caminho_bps: Option<u32>,
```

Na leitura da linha, o quinto campo é opcional: `campos.next().and_then(|c| c.parse().ok())`. Na escrita, um campo ausente não escreve tabulação sobrando — uma linha de quatro campos continua saindo de quatro.

- [ ] **Passo 4: rodar e ver passar**

Rodar: `cargo test -p seele-core conhecidos::`
Esperado: PASSA, e os testes antigos do formato continuam verdes.

- [ ] **Passo 5: escrever o teste da sonda que parte de uma medida**

```rust
#[test]
fn a_sonda_que_lembra_nao_recomeca_do_palpite() {
    // A escada existe porque ninguém sabe o cano. Quem já mediu **sabe**, e
    // recomeçar do palpite universal é jogar a medida fora.
    let lembrada = Sonda::partindo_de(12_480_000);
    assert_eq!(lembrada.estimativa(), 12_480_000);

    // E ela continua sendo uma sonda: um cano que encolheu a corrige na
    // primeira janela que doer, como corrigiria qualquer estimativa.
    let mut lembrada = lembrada;
    let mut cano = Cano::de(3_000_000);
    correr(&mut lembrada, &mut cano, SignalBand::Nominal, ticas(10), |_, _| {});
    assert!(
        lembrada.estimativa() < 12_480_000,
        "a memória virou teimosia: o cano encolheu e a estimativa não desceu"
    );

    // O piso continua sendo o piso: memória não autoriza começar acima do teto.
    assert_eq!(
        Sonda::partindo_de(u32::MAX).estimativa(),
        TETO_DA_ESTIMATIVA_BPS
    );
}
```

- [ ] **Passo 6: implementar**

```rust
    /// Uma sonda que começa de uma medida em vez do palpite de
    /// [`CAMINHO_DA_PROVA_BPS`].
    ///
    /// O valor é grampeado entre [`PISO_DA_ESTIMATIVA_BPS`] e
    /// [`TETO_DA_ESTIMATIVA_BPS`]: memória não é autorização para começar fora
    /// da faixa que a escada admite.
    ///
    /// **Não põe `limite_bps`.** Uma medida lembrada é de onde partir, não uma
    /// borda encontrada — a histerese é para o que doeu **nesta** sessão, e
    /// herdá-la de ontem impediria a sonda de descobrir que o cano cresceu.
    #[must_use]
    pub fn partindo_de(bps: u32) -> Self {
        Self {
            estimativa_bps: bps.clamp(PISO_DA_ESTIMATIVA_BPS, TETO_DA_ESTIMATIVA_BPS),
            ..Self::nova()
        }
    }
```

- [ ] **Passo 7: ligar as duas pontas**

Quem constrói a `Sonda` passa a ler o `Conhecido` do servidor em que está, e quem encerra uma transmissão passa a gravar `estimativa()` de volta na linha. O lugar de gravar é onde `crate::enlace` já vê `a transmissão de tela acabou` — é o único ponto que sabe que houve medida e que ela terminou.

Rodar: `cargo test -p seele-core`
Esperado: tudo verde.

- [ ] **Passo 8: conferir em campo, que é onde este defeito foi visto**

Duas máquinas na mesma LAN, compartilhar com movimento, esperar a escada chegar a P1080, parar, e compartilhar de novo. **A segunda vez tem de começar em 1080p**, sem os doze segundos.

- [ ] **Passo 9: commit**

```bash
git add crates/seele-core/src/conhecidos.rs crates/seele-core/src/caminho.rs
git commit -m "feat(tela): a escada lembra o caminho medido e para de reaprender a LAN"
```

