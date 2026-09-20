# Validação nativa da candidata API 4 — 20/09/2026

**Resultado: candidata ainda não aprovada para publicação.** O MESA voltou a
criar campanha e o PERFIS grava e apresenta os campos. O ESTILO, porém, não
consegue abrir o editor no aplicativo real. A faixa inferior continua presente
nos três MODs. Esta rodada registra execução no macOS, não apenas leitura de
código nem execução de laboratório.

## Ambiente e alcance

- SEELE: `d96d71a97805617077f5f61958dd9bfb85804ca5`.
- MESA: `41418ae54c1ae6cef369eefd1e53e1ea61e6f2b7`.
- PERFIS: `773d6f9671c2280f29478c79b13546dde301c94d`.
- ESTILO: `333739ed47de609e86dbd0ee80d611ddfa729ad5`.
- Três pacotes candidatos, versão `3.0.0`, API 4, gerados pelos respectivos
  `ferramentas/package.mjs` e instalados pela interface nativa.
- `cargo build -p seele-app --release`; binário colocado numa cópia de bundle
  em `target/qa-d96d71a/SEELE QA.app`, com assinatura local ad hoc e identificador
  `tech.datadev.seele.qa.d96d71a`. A etiqueta `0.13.0` desse bundle é de teste,
  não uma release publicada.
- `SEELE_HOME=/tmp/SEELE-QA-d96d71a/home`. Servidor hospedado no próprio app,
  `QA API 4 d96d71a`, uma pessoa (`pessoa-a504`), dados novos e descartáveis.
- Interações reais por acessibilidade e coordenadas, conferidas com capturas
  da janela. Dimensão observada: aproximadamente 1229 × 768.
- A instalação habitual e seus dados não foram substituídos. O servidor de
  teste foi encerrado pela interface ao fim. Os arquivos de QA foram preservados.

Também passou a bancada atual `contribuicoes-e-camadas.cjs`. Não repeti toda a
bateria Rust: a pergunta desta rodada era se as jornadas funcionavam na janela.

## Jornadas executadas

| Jornada | Resultado observado |
| --- | --- |
| Hospedar, instalar e ligar os três MODs | Funcionou; o app reconectou com o conjunto de três pacotes |
| MESA: criar `Campanha QA API 4` | Funcionou; revisão 1, campanha visível. O diálogo de criação ficou aberto e vazio |
| MESA: abrir página, criar e selecionar `Cena QA` | Funcionou; revisões 2 e 3 |
| MESA: rolar `1d20` e consultar Registro | Funcionou; revisão 4, `Dados · 1d20 = 8 [8]` |
| PERFIS: gravar nome, pronomes, status e biografia | Funcionou; retorno `Perfil gravado`, campos preservados ao reabrir |
| PERFIS: cartão na lista da direita | Apareceu depois de gravar. Antes de preencher, a substituição ficava vazia |
| PERFIS: abrir editor pelo cartão | Funcionou para o próprio perfil |
| PERFIS: fechar rascunho, cancelar, fechar novamente e descartar | Confirmação do produto por cima do MOD, interativa nos dois desfechos |
| ESTILO: abrir Aparência do servidor | **Falhou**: página vazia, `Falhou: fila-cheia`, depois `Não foi possível atualizar: fila-cheia` |
| Gestão: usar apresentação do SEELE para cartões | Nome nativo voltou, mas o cartão do MOD continuou anexado abaixo |
| Configurações: gestão de MODs e Microfone e som | Navegação por grupos e área ampla presentes; problemas de densidade/rolagem descritos abaixo |
| Sair do servidor | Voltou à entrada; páginas, regiões, cartões e entradas dos MODs desapareceram da tela |

A saída visual **não mede** liberação de memória, esvaziamento de todas as
tabelas ou ausência de tarefas nativas remanescentes. Não confundir esses aceites.

## Correções necessárias para esta candidata

### N1 — P1: ESTILO não cabe no transporte real

Reprodução: servidor novo, três candidatos ligados; clicar **Aparência do
servidor** na navegação. A página nasce, mas seu conteúdo não aparece. A região
inferior informa `fila-cheia`. A gestão posteriormente mostrou `o código não
carregou ... promessa rejeitada sem tratamento: fila-cheia`, embora o MOD já
tivesse carregado e mostrado sua região antes da abertura.

Depois da observação nativa, a medição de serialização com o prelúdio, cliente
e servidor reais encontrou **14.164 bytes** em `superficie-montar`. O teto em
`apps/seele-app/src/executor.rs:703` é **12.288 bytes**. Não é necessário encher
a fila: uma única mensagem já é recusada. O prelúdio converte qualquer `false`
de `postar` em `fila-cheia` (`executor.rs:882`).

Evidência reproduzível: `node docs/evidencias/medir-mensagem-estilo-d96d71a.cjs`.
Esse instrumento mede serialização em Node com respostas simuladas do
anfitrião; não é uma segunda execução nativa nem mede RAM/CPU.

**Entrega para o Claude:** montar apenas o conteúdo necessário da aba ativa
ou usar atualização incremental compatível com os limites, sem retirar os
tetos de contenção. Distinguir mensagem grande de saturação. Uma falha de
montagem deve deixar recuperação utilizável e diagnóstico coerente, não uma
página vazia e um estado de carregamento enganoso. Repetir no app: abrir,
selecionar conjunto, prévia, publicar e restaurar. Hoje esses passos posteriores
estão bloqueados; não foram aprovados.

### N2 — P1 de UX: a faixa permanente dos três MODs continua

Mesmo com entradas em **DOS MODS**, os três chamam `ui.regiao` e mantêm resumos
e botões no rodapé. Na janela observada a faixa ocupava cerca de 230 px antes
da falha do ESTILO, quase um terço da altura. A página do MESA perdeu espaço
para ela; a entrada Mesa ficou abaixo da área visível da navegação e o botão
Editar aparência ficou parcialmente cortado. As seis amostras do ESTILO se
empilharam verticalmente.

O wrapper `ferramentas/interface.js` dos pacotes ainda chama `desenhar` na
atualização periódica; `desenhar` usa `ui.regiao`. O produto ainda oferece
`.regioes-dos-mods` com teto de 240 px em `tela-sessao.css:2181`.

**Entrega:** nos pacotes API 4, abrir atividades pelas entradas, cartões e
ações contextuais; deixar a região legada como degradação da API 3 ou escolha
expressa, sem ocupar permanentemente a sessão dos três candidatos. Não basta
adicionar uma página se o formulário/resumo antigo continua tomando a janela.
Aceite: com todos ligados e nenhuma atividade aberta, conversa e navegação
usam a altura disponível; abrir uma atividade não mantém uma segunda cópia
fixa de sua entrada no rodapé.

### N3 — P2: PERFIS substitui pessoas sem ter conteúdo

Antes do primeiro preenchimento, a lista mostrava um alvo de perfil vazio e
`DETALHES · pessoa-a504 (você)`. Depois de salvar, avatar com inicial, nome,
pronomes e status apareceram corretamente. Portanto não é ausência total de
integração: é o caminho de pessoa sem perfil.

`cartaoDaPessoa` em `SEELE-MOD-PERFIS/ferramentas/perfis.js` retorna `null` sem
campos preenchidos, enquanto `registrarApresentacao` registra a substituição
geral. `linhaSubstituida`, em `tela-sessao.js:1818`, cria o botão mesmo sem
cartão. Seu nome acessível genérico também não distingue a pessoa.

**Entrega:** apresentar identidade e inicial nativas quando faltar conteúdo,
sem retirar o acesso ao editor; nome acessível deve identificar a pessoa.
Aceite com uma pessoa sem perfil e outra preenchida na mesma lista.

### N4 — P2: escolha de apresentação nativa ainda deixa o cartão legado

Na gestão, **O cartão de cada pessoa → USAR APRESENTAÇÃO DO SEELE** restaurou
`pessoa-a504 (você)` e o diagnóstico nativo, mas manteve `Perfil QA`, pronomes
e status num cartão logo abaixo. A preferência alcança a substituição;
`cartoesDaPessoa`, em `base.js:926`, continua devolvendo os cartões legados
para a linha nativa.

**Entrega:** fazer a escolha explícita desse ponto valer também para o
caminho legado do mesmo provedor. Preservar compatibilidade da API 3 não deve
ignorar a preferência do usuário. Aceite: selecionar nativo, observar somente
a apresentação nativa desse ponto, depois selecionar o MOD e observar sua
volta, sem duplicação.

### N5 — P2: criar campanha deixa o formulário de criação vazio e aberto

Após **CRIAR MESA**, a campanha foi criada e o resumo mudou, mas o diálogo
continuou com nome vazio e botão desabilitado. Foi necessário fechá-lo
manualmente para abrir a campanha. Em `ferramentas/mesa.js:1429`, o sucesso
limpa o rascunho e redesenha; esse caminho não fecha o diálogo de criação.

**Entrega:** após sucesso, fechar a criação e abrir a campanha, com feedback
claro. Em erro, preservar valores e explicar a recusa. Não voltar ao problema
anterior: a escrita no servidor funcionou nesta rodada.

### N6 — P2: compositor nativo ficou um textarea branco e estreito

O chat mostra uma moldura escura larga com um pequeno campo branco de
aproximadamente 145 px dentro. Placeholder truncado e aparência de controle
padrão do WebKit. `index.html:1595` agora usa `textarea`; a regra de expansão,
fundo e fonte em `tela-sessao.css:1144` ainda seleciona `.compor input`.
`#campo-mensagem` só complementa altura, rolagem e line-height.

**Entrega:** aplicar ao textarea o estilo e expansão do compositor e conferir
uma e várias linhas, com o botão Enviar visível. Esta é uma regressão no
produto base encontrada durante a jornada, não uma falha do código do MOD.

## Configurações: avanço confirmado e acabamento que falta

A área ampla e os grupos **Este dispositivo / MODs / Este servidor /
Aplicativo** são melhores que o modal anterior com assuntos misturados. A
gestão tem abas próprias. A mudança está presente no aplicativo.

Ainda há excesso de texto permanente no menu lateral: título, parágrafo de
introdução e explicações de cada grupo fazem **Este servidor** ficar perto do
fim da tela e **Aplicativo** exigir rolagem. Na área de áudio há grandes
espaços e duas instruções separadas para a mesma decisão de dispositivo. Ao
trocar da gestão de MODs para Microfone e som, a área abriu com o começo do
conteúdo acima da região visível; rolar para cima recuperou os títulos.

Próxima melhoria delimitada: menu com rótulos curtos; explicações na seção
aberta; dicas secundárias recolhidas; reduzir espaços entre instruções;
restaurar rolagem por seção ou começar no topo, evitando herdar uma posição
sem contexto. No teste, o cabeçalho também dizia «microfone aberto» enquanto
o modo selecionado era **TECLA**: separar estado do dispositivo, modo de
ativação e transmissão efetiva na redação.

## O que esta rodada não aprovou

Não foram executados: segundo participante e autorizações entre pessoas,
ficha de terceiro/privacidade, upload/recorte de retrato e faixa, arraste de
peças, mídia, todas as combinações de Tab/Shift+Tab e ordem inversa de modais,
memória/CPU com voz e ciclos repetidos, Windows e Linux. Escape foi enviado
uma vez no editor com rascunho e não fechou; o botão Fechar abriu a confirmação.
Isso pede uma verificação de teclado específica, sem atribuir uma causa ainda
não demonstrada.

**Ordem para encerrar esta entrega:** corrigir N1 e N2; fechar N3–N6; repetir
somente as jornadas afetadas no aplicativo. Depois completar os aceites
pendentes já definidos no plano. Não ampliar a API nem reabrir toda a
arquitetura para corrigir estes problemas concretos. Nada foi publicado,
empurrado ou alterado no código de produção por esta validação.
