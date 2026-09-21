# A escolha de qualidade no compartilhamento — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A resolução escolhida passa a ser o **piso** da estimativa de caminho
além do teto de qualidade, e a caixa de compartilhar passa a perguntar quadros
além de resolução.

**Architecture:** Uma mudança em `seele-core` (o piso da perna do hospedeiro sai
da escolha enquanto o `HostUplink` for zero) e uma na casca (um segundo
`<select>`). O fio, o servidor e a escada adaptativa não mudam.

**Tech Stack:** Rust (`seele-core`), JavaScript e HTML sem build
(`apps/seele-app/ui`), testes de texto-fonte em `apps/seele-app/tests/frontend.rs`,
bancada em Chromium via Playwright (`apps/seele-app/bancada`).

**Spec:** `docs/superpowers/specs/2026-09-21-escolha-de-qualidade-no-compartilhamento-design.md`

## Global Constraints

- **Português nos comentários e nas mensagens de teste**, como o resto do
  repositório. A mensagem de uma asserção diz **o que quebra**, não «falhou».
- **Nada de dependência nova.** `apps/seele-app/Cargo.toml` diz «nada além
  disso».
- **Todo guarda é provado por reversão** antes de ser declarado guarda: desfaz o
  conserto, vê o teste falhar, restaura. É o «existir não é funcionar» do
  `CLAUDE.md`.
- **O fio não muda.** `caminho_no_fio` continua sem pôr hipótese nele.
- **`CAMINHO_DO_SERVER_BPS` continua em 2 Mbps.** Ele foi revertido nesta mesma
  onda porque cegava a medida, e tem guarda próprio
  (`a_hipotese_deixa_o_piso_demonstrado_disparar`).
- Antes de qualquer commit: `cargo fmt --all`, e
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` limpo.

---

### Task 1: `caminho_inicial_bps` passa a derivar do limiar

Hoje ela devolve 3, 5 e 8 Mbps escritos à mão, e o de 1080p **não compra 1080p**:
8 Mbps dão teto de 4,8 e o limiar pede 6,24. Derivar do limiar faz os três
comprarem o que prometem por construção.

**Files:**
- Modify: `crates/seele-core/src/video.rs:744-753`
- Test: `crates/seele-core/src/video.rs` (módulo `tests` do próprio arquivo)

**Interfaces:**
- Consumes: `crate::tela::bits_por_quadro(Resolucao) -> u32`,
  `crate::tela::CADENCIA_DE_REFERENCIA: u32`, `crate::tela::FRACAO_DO_CAMINHO: u32`
- Produces: `LimitesDeTela::caminho_inicial_bps(&self) -> u32` — mesma
  assinatura, valores novos: 2 600 000 / 4 650 000 / 10 400 000

- [ ] **Step 1: Escrever o teste que falha**

No módulo `tests` de `crates/seele-core/src/video.rs`:

```rust
/// **Cada piso compra a resolução que promete.**
///
/// O piso existe para pular a escada, e um piso que não alcança o próprio
/// degrau não pula nada: quem escolhe 1080p continua em 720p, em silêncio.
/// Foi o que os 8 000 000 escritos à mão faziam — 4,8 Mbps de teto contra um
/// limiar de 6,24.
///
/// Irmão de `cada_limiar_compra_a_resolucao_que_promete`, que guarda a outra
/// ponta da mesma tabela.
#[test]
fn cada_piso_inicial_compra_a_resolucao_que_promete() {
    for resolucao in [Resolucao::P540, Resolucao::P720, Resolucao::P1080] {
        let limites = LimitesDeTela {
            resolucao,
            ..LimitesDeTela::default()
        };
        let piso = limites.caminho_inicial_bps();
        let teto = (u64::from(piso) * u64::from(crate::tela::FRACAO_DO_CAMINHO) / 100) as u32;
        assert_eq!(
            crate::tela::resolucao_para(teto, crate::tela::Prioridade::Nitidez),
            resolucao,
            "o piso de {resolucao:?} são {piso} bps, que dão teto de {teto} e \
             compram outro degrau: quem escolher {resolucao:?} continua no degrau \
             de baixo, que é o pedido inteiro não atendido"
        );
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core --lib video::tests::cada_piso_inicial_compra`
Expected: FAIL em `P1080` — «compram outro degrau».

- [ ] **Step 3: Derivar do limiar**

Substituir o corpo em `crates/seele-core/src/video.rs:747-753`:

```rust
    /// De que caminho a sonda parte quando esta resolução foi escolhida.
    ///
    /// **Derivado do limiar, e não escrito à mão.** Os três números eram
    /// 3, 5 e 8 Mbps, e o de 1080p ficava 2,4 Mbps abaixo do que o próprio
    /// degrau custa: 8 Mbps dão teto de 4,8 e o limiar são 6,24. Quem escolhia
    /// 1080p continuava em 720p — a opção inteira, decorativa.
    ///
    /// É a armadilha que `TETO_ESTIMADO_PARA_1080P_BPS` já escreve sobre si
    /// mesma: *«duas constantes arredondadas em separado divergem; uma derivada
    /// da outra não pode»*. Aqui a divergência era entre o piso e o limiar que
    /// ele existe para alcançar.
    ///
    /// A conta é o inverso de `fracao_do`: se o vídeo leva
    /// `FRACAO_DO_CAMINHO` por cento do caminho, o caminho que compra um
    /// limiar é o limiar dividido por essa fração.
    #[must_use]
    pub const fn caminho_inicial_bps(&self) -> u32 {
        let limiar =
            crate::tela::bits_por_quadro(self.resolucao) * crate::tela::CADENCIA_DE_REFERENCIA;
        ((limiar as u64 * 100) / crate::tela::FRACAO_DO_CAMINHO as u64) as u32
    }
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p seele-core --lib video::tests::cada_piso_inicial_compra`
Expected: PASS.

Depois `cargo test -p seele-core --lib` inteiro: a mudança move a perna de quem
compartilha, e se algum teste de `caminho::` ou `tela::` pinava 3/5/8 Mbps ele
aparece aqui. Se aparecer, **leia antes de mexer**: pode ser um número honesto
que mudou, e aí a expectativa muda com a razão escrita no teste.

- [ ] **Step 5: Provar por reversão**

Voltar `P1080 => 8_000_000` à mão, rodar o teste, confirmar que ele falha com a
frase «compram outro degrau», restaurar.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/seele-core/src/video.rs
git commit
```

---

### Task 2: O piso da perna do hospedeiro

**Files:**
- Modify: `crates/seele-core/src/enlace.rs:4375-4382` (a função), e os três
  chamadores em `:4340`, `:4401`, `:4425`
- Test: `crates/seele-core/src/enlace.rs` (módulo `tests`)

**Interfaces:**
- Consumes: `LimitesDeTela::caminho_inicial_bps()` da Task 1
- Produces: `Motor::teto_de_video(&self, limites: Option<&crate::video::LimitesDeTela>) -> TetoDeVideo`

- [ ] **Step 1: Escrever os três testes que falham**

```rust
/// **Sem `HostUplink`, a perna do hospedeiro sai da escolha.**
///
/// É o pedido inteiro: escolher 1080p e não esperar a escada subir de 2 Mbps.
#[test]
fn sem_host_uplink_a_perna_do_hospedeiro_sai_da_escolha() {
    let mut motor = motor_de_teste();
    // A perna de quem compartilha sai da frente, para o teto revelar a do
    // hospedeiro sozinha. Em produção ela também parte da escolha — o
    // `Comando::CompartilharTela` semeia a sonda com o mesmo número —, e aí as
    // duas dão o mesmo valor; aqui ela é posta alta de propósito para que este
    // teste falhe pelo motivo que ele nomeia, e não por causa da outra perna.
    motor.caminho = crate::caminho::Sonda::partindo_de(100_000_000);
    motor.caminho_de_quem_hospeda_bps = None;
    let limites = LimitesDeTela {
        resolucao: Resolucao::P1080,
        ..LimitesDeTela::default()
    };

    let teto = motor.teto_de_video(Some(&limites)).teto(SignalBand::Nominal);

    // 10 400 000 de piso × 60% = 6 240 000, que é exatamente o limiar de 1080p.
    assert_eq!(
        teto,
        Teto::Bps(6_240_000),
        "escolher 1080p sem medida nenhuma do hospedeiro tinha de abrir no teto \
         que 1080p custa; abriu em {teto:?} — a escolha só passa a valer oito \
         janelas depois, que é o que este piso existe para não fazer"
    );
}

/// **Uma medida baixa desmente a escolha, e vence.**
///
/// É o lado que importa: repetir o palpite por cima de um número que a máquina
/// já mediu seria insistir no que ela sabe estar errado. O lado alto está
/// coberto pelo teste acima — lá o piso é o que manda porque não há medida
/// nenhuma.
#[test]
fn a_medida_baixa_do_hospedeiro_vence_o_piso_da_escolha() {
    let limites = LimitesDeTela {
        resolucao: Resolucao::P1080,
        ..LimitesDeTela::default()
    };
    let mut motor = motor_de_teste();
    // A perna de quem compartilha sai da frente, para o teto revelar a do
    // hospedeiro sozinha — o espelho do que
    // `o_teto_sai_do_caminho_que_a_sonda_mediu_e_nao_da_suposicao` faz com a
    // outra perna.
    motor.caminho = crate::caminho::Sonda::partindo_de(100_000_000);
    // Um mega medido: muito abaixo dos 10,4 que 1080p pediria de palpite.
    motor.caminho_de_quem_hospeda_bps = Some(1_000_000);

    let teto = motor.teto_de_video(Some(&limites)).teto(SignalBand::Nominal);

    assert_eq!(
        teto,
        Teto::Bps(600_000),
        "com 1 Mbps medidos do hospedeiro o teto saiu {teto:?}: o piso da \
         escolha passou por cima de uma medida de verdade, e o produto voltou a \
         prometer banda que ninguém conferiu"
    );
}

/// **Fora de uma transmissão não há escolha**, e o padrão segue a suposição.
#[test]
fn sem_transmissao_o_padrao_do_hospedeiro_segue_a_suposicao() {
    let mut motor = motor_de_teste();
    motor.caminho_de_quem_hospeda_bps = None;
    let teto = motor.teto_de_video(None).teto(SignalBand::Nominal);
    assert_eq!(
        teto,
        Teto::Bps(1_200_000),
        "sem escolha nenhuma o teto deixou de ser o de CAMINHO_DA_PROVA_BPS"
    );
}
```

**Nota para quem implementa:** `motor_de_teste()` já existe no módulo `tests`
deste arquivo e é o mesmo ajudante que
`o_teto_sai_do_caminho_que_a_sonda_mediu_e_nao_da_suposicao` (em `:6168`) usa.
Os `use` que os três testes pedem — `crate::tela::Teto`,
`crate::video::{LimitesDeTela, Resolucao}`, `seele_proto::signal::SignalBand` —
seguem o que aquele teste já importa.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core --lib enlace::tests::sem_host_uplink`
Expected: FAIL de compilação — `teto_de_video` ainda recebe `Option<u32>`.

- [ ] **Step 3: Trocar a assinatura e aplicar o piso**

Em `crates/seele-core/src/enlace.rs:4375`:

```rust
    fn teto_de_video(
        &self,
        limites: Option<&crate::video::LimitesDeTela>,
    ) -> crate::tela::TetoDeVideo {
        let mut teto = crate::tela::TetoDeVideo::com_caminho(self.caminho.estimativa());
        // **A escolha é piso enquanto ninguém mediu, e teto sempre.**
        //
        // `caminho_de_quem_hospeda_bps` é `Some` só quando o `HostUplink` veio
        // maior que zero — medido ou declarado. Havendo número real, ele manda,
        // para mais e para menos: repetir o palpite por cima de uma medida
        // seria insistir no que a máquina já desmentiu.
        //
        // Sem número real, o padrão deixa de ser `CAMINHO_DA_PROVA_BPS` e passa
        // a sair da resolução escolhida — senão a primeira transmissão de uma
        // sessão abre em 540p qualquer que seja a escolha, e a escolha só
        // começa a valer oito janelas depois.
        //
        // Fora de uma transmissão não há escolha, e aí a suposição volta.
        let hospeda = self
            .caminho_de_quem_hospeda_bps
            .or_else(|| limites.map(crate::video::LimitesDeTela::caminho_inicial_bps));
        if let Some(bps) = hospeda {
            teto = teto.com_caminho_de_quem_hospeda(bps);
        }
        teto.com_espectadores(self.espectadores)
            .com_escolha(limites.and_then(|l| l.banda_bps))
    }
```

- [ ] **Step 4: Acertar os três chamadores**

- `:4340` — `teto: self.teto_de_video(Some(&limites)),`
- `:4401` — `teto: self.teto_de_video(Some(&viva.limites)).teto(self.faixa),`
- `:4425` — `let teto = self.teto_de_video(Some(&viva.limites));`

Os chamadores dentro de `tests` que passam `None` continuam como estão: eles
fixam `caminho_de_quem_hospeda_bps = Some(...)` antes, então o piso não os
alcança.

- [ ] **Step 5: Rodar e ver passar**

Run: `cargo test -p seele-core --lib enlace::`
Expected: PASS, os três novos e os que já existiam.

- [ ] **Step 6: Provar por reversão**

Trocar o `.or_else(...)` por `.or(None)`, rodar, confirmar que
`sem_host_uplink_a_perna_do_hospedeiro_sai_da_escolha` falha dizendo que abriu
em 1 200 000. Restaurar.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p seele-core --lib
git add crates/seele-core/src/enlace.rs
git commit
```

---

### Task 3: Quadros na caixa de compartilhar

**Files:**
- Modify: `apps/seele-app/ui/index.html:3805-3812`
- Modify: `apps/seele-app/ui/camada-compartilhar.js:403-420`
- Test: `apps/seele-app/tests/frontend.rs` (perto de `:10054`)
- Test: `apps/seele-app/bancada/ajustes-v013.cjs:30-40`

**Interfaces:**
- Produces: elemento `#compartilhar-quadros`; `limitesEscolhidos()` passa a
  devolver `quadros_maximos` lido dele

- [ ] **Step 1: Escrever o guarda de texto-fonte que falha**

Em `apps/seele-app/tests/frontend.rs`, ao lado do guarda de `prioridade`:

```rust
/// **Os quadros saem do seletor, e não de um número fixo.**
///
/// Eles eram `60` escrito na função. O espelho do guarda de `prioridade`, logo
/// acima: uma escolha que a caixa oferece e a ponte ignora é pior que uma
/// escolha que não existe — a pessoa mexe, nada muda, e não há erro nenhum a
/// mostrar.
#[test]
fn os_quadros_escolhidos_saem_do_seletor() {
    let limites = js_function(&scripts(), "function limitesEscolhidos(");
    assert!(
        limites.contains("compartilhar-quadros"),
        "`limitesEscolhidos` não lê o seletor de quadros: {limites}"
    );
    assert!(
        !limites.contains("quadros_maximos: 60"),
        "`quadros_maximos` voltou a ser um número fixo: {limites}"
    );
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-app --test frontend os_quadros_escolhidos`
Expected: FAIL — «não lê o seletor de quadros».

- [ ] **Step 3: O seletor no HTML**

Depois do bloco de resolução em `apps/seele-app/ui/index.html:3812`:

```html
      <div class="compartilhar-escolha">
        <label class="rotulo" for="compartilhar-quadros">QUADROS</label>
        <select id="compartilhar-quadros" class="compartilhar-select">
          <option value="30">30 por segundo</option>
          <option value="60" selected>60 por segundo</option>
        </select>
      </div>
```

`Q8` e `Q15` ficam de fora: existem como piso automático, para onde a rede desce
pela regra «a resolução segura, o quadro cede». Oferecer 8 a quem escolhe é
pedir que a pessoa escolha «propositalmente travado».

- [ ] **Step 4: A leitura e o ouvinte**

Em `apps/seele-app/ui/camada-compartilhar.js`, na função `limitesEscolhidos`:

```js
    quadros_maximos: Number($("compartilhar-quadros").value) || 60,
```

E o ouvinte de `change`, que hoje está só na resolução, passa a servir aos dois.
Extrair o corpo para uma função nomeada e registrá-la nos dois elementos:

```js
/** A troca vale durante a transmissão: os dois seletores entram por aqui. */
async function aplicarEscolhaDeQualidade() {
  erroDeTela = null;
  // ... o corpo que hoje está no ouvinte da resolução, sem mudança
}

$("compartilhar-resolucao").addEventListener("change", aplicarEscolhaDeQualidade);
$("compartilhar-quadros").addEventListener("change", aplicarEscolhaDeQualidade);
```

- [ ] **Step 5: Rodar os testes de texto-fonte**

Run: `cargo test -p seele-app --test frontend`
Expected: PASS, inclusive `every_element_the_script_reaches_for_exists_in_the_page`
e `toda_classe_que_o_script_aplica_tem_regra_de_css`, que alcançam o elemento
novo sozinhos.

- [ ] **Step 6: A bancada, com a transmissão no ar**

Em `apps/seele-app/bancada/ajustes-v013.cjs`, ao lado da checagem de resolução:

```js
  await page.selectOption('#compartilhar-quadros', '30');
  assert.equal(await page.evaluate(() => limitesEscolhidos().quadros_maximos), 30);
  await page.selectOption('#compartilhar-quadros', '60');
  assert.equal(await page.evaluate(() => limitesEscolhidos().quadros_maximos), 60);
```

Run: `node apps/seele-app/bancada/ajustes-v013.cjs`
Expected: a linha de sempre, sem exceção.

- [ ] **Step 7: Provar por reversão**

Voltar `quadros_maximos: 60` fixo, rodar `cargo test -p seele-app --test frontend
os_quadros_escolhidos` e a bancada, confirmar que os dois falham. Restaurar.

- [ ] **Step 8: Commit**

```bash
git add apps/seele-app/ui/index.html apps/seele-app/ui/camada-compartilhar.js \
        apps/seele-app/tests/frontend.rs apps/seele-app/bancada/ajustes-v013.cjs
git commit
```

---

### Task 4: O padrão da caixa passa a ser 720p

Com o piso, o padrão deixa de ser preferência e vira **suposição sobre a casa de
quem hospeda**. A 1080p ele suporia 10,4 Mbps de todo mundo que nunca abrisse a
caixa.

**Files:**
- Modify: `apps/seele-app/ui/index.html:3808-3810`
- Test: `apps/seele-app/bancada/ajustes-v013.cjs`

- [ ] **Step 1: Escrever a asserção que falha**

Em `apps/seele-app/bancada/ajustes-v013.cjs`, antes de qualquer `selectOption`:

```js
  assert.equal(
    await page.evaluate(() => document.getElementById('compartilhar-resolucao').value),
    '720',
    'a caixa deixou de nascer em 720p: com o piso, o padrão supõe a subida da ' +
      'casa de quem hospeda, e 1080p suporia 10,4 Mbps de quem nunca abriu a caixa',
  );
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `node apps/seele-app/bancada/ajustes-v013.cjs`
Expected: FAIL — o valor é `1080`.

- [ ] **Step 3: Mover o `selected`**

```html
          <option value="540">540p</option>
          <option value="720" selected>720p</option>
          <option value="1080">1080p</option>
```

- [ ] **Step 4: Rodar e ver passar**

Run: `node apps/seele-app/bancada/ajustes-v013.cjs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/seele-app/ui/index.html apps/seele-app/bancada/ajustes-v013.cjs
git commit
```

---

### Task 5: As notas da v0.14.0 contam isto

**Files:**
- Modify: `docs/notas-da-v0.14.0.md` (a seção «O compartilhamento de tela
  voltou a ter uma escolha»)

- [ ] **Step 1: Reescrever a seção**

Dizer, na ordem: que a caixa pergunta resolução **e** quadros; que a escolha
passou a ser piso além de teto, e o que isso significa — a primeira
transmissão abre no degrau escolhido em vez de subir a escada; que havendo
medida lembrada ou subida declarada, elas vencem; que o padrão é 720p e por quê;
e que o piso de 1080p estava curto e agora deriva do limiar.

- [ ] **Step 2: A bateria inteira, capturada**

```bash
cargo test --workspace -- --test-threads=1 > /tmp/bateria.log 2>&1
echo "CODIGO_DO_CARGO=$?"
```

**Ler o código de saída do `cargo`, não o de um cano.** `cargo ... | tail` devolve
o código do `tail`, que é sempre zero — foi o que quase deixou uma bateria
reprovada passar por verde nesta mesma onda.

Expected: `CODIGO_DO_CARGO=0`, e `0 failed` somado sobre os binários.

- [ ] **Step 3: As bancadas**

```bash
for b in gif-na-conversa ajustes-v013 contribuicoes-e-camadas continuacao-de-midia envio-de-imagens regiao-do-mod; do
  node apps/seele-app/bancada/$b.cjs || echo "FALHOU: $b"
done
```

- [ ] **Step 4: Commit**

```bash
git add docs/notas-da-v0.14.0.md
git commit
```

---

## O que este plano deliberadamente não faz

- **Não separa `CAMINHO_DO_SERVER_BPS` em dois números.** O portão admite seis
  cópias a 2 Mbps; a separação fica para quando alguém medir que ele atrapalha.
- **Não põe a subida declarada na interface.** `config.caminho_bps` já atravessa
  o fio e já vence o piso; falta só o campo, e é outra entrega.
- **Não mexe na sonda.** Ela acabou de custar uma bateria reprovada nesta onda.
- **Não oferece 8 e 15 quadros.**
