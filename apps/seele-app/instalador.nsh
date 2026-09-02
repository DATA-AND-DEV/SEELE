; Ganchos do instalador NSIS. Ver `bundle.windows.nsis.installerHooks`.
;
; # Por que este arquivo existe
;
; Até a 0.7.1 o SEELE se instalava por usuário, em `%LOCALAPPDATA%\SEELE`. A
; 0.7.2 passou a instalar para a máquina, em `Program Files`, porque ninguém
; achava o app onde as pessoas procuram — e porque sem elevação o instalador não
; consegue criar a regra de firewall de entrada.
;
; O que ninguém viu na hora: **o instalador novo não enxerga o antigo.** Ele
; procura a instalação anterior no `HKLM`, que é o registro da máquina, e a
; antiga mora no `HKCU`, que é o do usuário. O resultado é uma segunda cópia no
; `Program Files` enquanto a do `AppData` continua inteira, com os atalhos do
; menu Iniciar e da área de trabalho ainda apontando para ela.
;
; Daí o sintoma relatado: **o aplicativo volta de versão.** O atualizador roda o
; instalador, que atualiza a cópia do `Program Files`, e o atalho continua
; abrindo a do `AppData` — parada na versão em que foi deixada.

!macro NSIS_HOOK_PREINSTALL
  ; A instalação por usuário, se houver. `HKCU` explícito e não `SHCTX`: neste
  ; instalador o contexto é o da máquina, que é justamente onde ela não está.
  ReadRegStr $R7 HKCU "${UNINSTKEY}" "UninstallString"
  ${If} $R7 != ""
    ; A pasta vem daqui e não de `InstallLocation`: aquele é gravado com aspas
    ; dentro do próprio valor, e usá-lo como caminho não funciona. Este é o
    ; mesmo registro que o Tauri lê para reencontrar uma instalação anterior.
    ReadRegStr $R8 HKCU "${MANUPRODUCTKEY}" ""
    ${If} $R8 != ""
    ${AndIf} ${FileExists} "$R8\uninstall.exe"
      ; `/S` e **não** `/UPDATE`. São dois efeitos e os dois importam: `/S` não
      ; marca a caixa de apagar dados — ela nasce vazia e a página que a marca
      ; não roda no modo silencioso —, então o PERSISTENCE, a identidade e os pinos
      ; do ADR 0003 ficam onde estão; e a ausência de `/UPDATE` é o que faz o
      ; desinstalador remover os atalhos, que é o ponto inteiro deste gancho.
      ;
      ; `_?=` faz o desinstalador rodar na própria pasta em vez de se copiar
      ; para o temporário. É o que faz o `ExecWait` esperar de verdade: sem
      ; isso ele devolve na hora e a instalação nova corre junto com a remoção
      ; da antiga.
      ;
      ; Isto acontece antes do `CheckIfAppIsRunning` do próprio modelo, então o
      ; executável antigo pode estar aberto e não sair. Tudo bem: os atalhos e o
      ; registro saem, e são eles que faziam a versão voltar. O que sobra é uma
      ; pasta órfã que ninguém mais abre.
      ExecWait '"$R8\uninstall.exe" /S _?=$R8'
      Delete "$R8\uninstall.exe"
      RMDir "$R8"
    ${Else}
      ; Registro sem desinstalador: alguém apagou a pasta à mão. Some com a
      ; entrada, ou ela fica aparecendo em "Aplicativos instalados" para sempre.
      DeleteRegKey HKCU "${UNINSTKEY}"
    ${EndIf}
  ${EndIf}
!macroend

; # A regra de firewall de entrada
;
; No macOS e no Linux o firewall padrão não barra entrada de um programa que já
; está escutando. No Windows barra: a regra nasce quando alguém clica «permitir»
; num diálogo que aparece uma vez, na primeira execução — e não nasce se a
; pessoa apertar Cancelar, ou se a rede estiver marcada como pública, que é o
; que o Windows faz sozinho com metade das redes domésticas. Sem a regra o
; anfitrião sobe o servidor, vê tudo verde do lado dele, e ninguém entra.
;
; Até aqui o SEELE só sabia **olhar** para essa regra: `alcance::firewall` lê a
; saída do `netsh` e responde se há uma. Ele se recusava a criar, e a razão
; escrita no cabeçalho dele era que o instalador rodava por usuário e não tinha
; elevação. Essa razão caducou na 0.7.2, quando a instalação passou a ser da
; máquina — e o comentário no topo deste arquivo diz que ela passou a ser da
; máquina **por causa disto**. A elevação foi conquistada para criar a regra e a
; regra nunca foi criada.
;
; ## As escolhas
;
; **Por programa e não por porta.** `program=` e não `localport=8383`: a porta
; do encontro é a 8384, ao lado da do servidor, e uma regra por porta deixaria
; metade do problema de pé. É também a forma que `alcance::firewall::ha_regra_para`
; sabe reconhecer — ele compara a linha `Program`, então uma regra por porta
; seria invisível para o próprio código que confere.
;
; **`profile=any`.** É o que todos os nossos documentos mandam a pessoa colar à
; mão, e é o que resolve a rede doméstica que o Windows classificou como
; pública — que é metade do sintoma. Vale dizer o que isso é: mais permissivo
; que o diálogo do Windows, que vem com «pública» desmarcada. O que segura o
; risco é o escopo, que é um executável em UDP, e não uma porta aberta para
; qualquer programa.
;
; **Apagar antes de criar.** O `netsh` aceita duas regras com o mesmo nome sem
; reclamar, e este gancho roda de novo a cada atualização: sem o `delete` a
; lista de regras cresceria uma por versão. O `delete` é por nome e não por
; caminho de propósito — é o que varre também a regra da instalação por usuário
; antiga, que aponta para um `.exe` no `AppData` que não existe mais.
;
; **Falhar aqui não derruba a instalação.** O código de saída do `netsh` é
; retirado da pilha e ignorado: quem não conseguiu a regra ainda tem um app que
; funciona para entrar nos servidores dos outros, e trocar isso por uma
; instalação abortada seria péssimo negócio. A saída do `netsh` fica no log do
; instalador, atrás de «mostrar detalhes».
;
; `$SYSDIR\netsh.exe` com caminho inteiro, e não `netsh` solto: este gancho roda
; elevado, e resolver o nome pelo PATH num processo com administrador é entregar
; a quem escreve no PATH o direito de escolher o que roda.

!macro NSIS_HOOK_POSTINSTALL
  ; No modo silencioso não houve página, então a escolha vem do registro — ver
  ; `SeeleLeAsEscolhas`. É o caminho que o atualizador percorre, e o único em que
  ; ninguém está olhando.
  IfSilent 0 +2
    Call SeeleLeAsEscolhas

  WriteRegDWORD SHCTX "${SEELE_ESCOLHAS}" "FirewallUDP" $SeeleQuerFirewall
  WriteRegDWORD SHCTX "${SEELE_ESCOLHAS}" "AtalhoNaAreaDeTrabalho" $SeeleQuerAtalho

  ; A regra sai sempre antes, marcada ou não: sem isso, desmarcar a caixa numa
  ; reinstalação deixaria de pé a regra da instalação anterior, e a caixa teria
  ; mentido.
  nsExec::ExecToLog '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="SEELE" dir=in'
  Pop $0
  ${If} $SeeleQuerFirewall == 1
    nsExec::ExecToLog '"$SYSDIR\netsh.exe" advfirewall firewall add rule name="SEELE" dir=in action=allow protocol=udp profile=any enable=yes program="$INSTDIR\${MAINBINARYNAME}.exe" description="Deixa entrar conexão para o servidor SEELE hospedado nesta máquina."'
    Pop $0
  ${EndIf}

  ${If} $SeeleQuerAtalho == 1
    Call CreateOrUpdateDesktopShortcut
  ${EndIf}
!macroend

; Sai junto com o programa. Numa atualização o desinstalador antigo roda antes
; do instalador novo, que recria a regra no gancho de cima — a ordem converge
; sozinha e não precisa perguntar se é atualização ou remoção de verdade.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="SEELE" dir=in'
  Pop $0
!macroend

; ============================================================================
; A cara do instalador.
;
; O desenho está em `Instalador SEELE.dc.html`, do projeto de design, e o que
; chega aqui é o que o NSIS sabe desenhar. O que **não** chega, e é decisão
; registrada e não esquecimento: a barra de título continua sendo a do Windows,
; e os botões do rodapé continuam sendo os nativos — o Windows os desenha pelo
; tema do sistema e `SetCtlColors` não os alcança. Um botão pintado à mão custaria
; desenhar estado de foco, de pressionado e de desabilitado, e um botão que não
; mostra foco é um instalador que ninguém navega pelo teclado.
;
; Este arquivo carrega quase tudo de propósito: ele é **nosso**, enquanto o
; `instalador.nsi` é bifurcado e paga imposto a cada versão do Tauri. Quanto mais
; fina a bifurcação, mais barato rebifurcar.
; ============================================================================

; As cores de `apps/seele-app/ui/tokens.css`. Em 0xRRGGBB, que é como o
; `SetCtlColors` as quer — e não em BGR, que é como o BMP as guarda.
!define SEELE_NEGRO   0x050403
!define SEELE_PAINEL  0x0A0806
!define SEELE_LINHA   0x241F19
!define SEELE_OSSO    0xEAE3CF
!define SEELE_ROTULO  0x908574
!define SEELE_LARANJA 0xF2521F

Var SeeleDialogo
Var SeeleFonteCartela
Var SeeleFonteRotulo
Var SeeleMarca
Var SeeleCaixaAtalho
Var SeeleCaixaFirewall
Var SeeleQuerAtalho
Var SeeleQuerFirewall

!define MUI_CUSTOMFUNCTION_GUIINIT SeeleAcertaAJanela

; Pinta a moldura que o MUI desenha antes de qualquer página existir.
;
; Os números são os ids que o MUI dá aos controles da janela de fora. Eles não
; têm nome em lugar nenhum — estão no `Contrib\Modern UI 2` do NSIS — e por isso
; vão nomeados aqui: 1034 a 1036 são o fundo do cabeçalho e a régua, 1037 o
; título, 1038 o subtítulo, 1256 a linha de marca do rodapé.
Function SeeleAcertaAJanela
  SetCtlColors $HWNDPARENT ${SEELE_OSSO} ${SEELE_NEGRO}

  ; A cartela e o rótulo miúdo. `Saira Condensed` não está instalada em máquina
  ; nenhuma e este instalador não vai instalar fonte para desenhar a si mesmo —
  ; então a cartela é a condensada que todo Windows tem, que é o que a própria
  ; `--seele-display` já declara como alternativa.
  CreateFont $SeeleFonteCartela "Arial Narrow" "14" "700"
  CreateFont $SeeleFonteRotulo "Segoe UI" "7" "400"

  ; A marca sai do instalador para o temporário, uma vez, antes de qualquer
  ; página. `${SIDEBARIMAGE}` é o caminho que o bundler resolveu ao copiar o
  ; arquivo para junto do modelo gerado — daqui não há caminho relativo que
  ; sobreviva.
  InitPluginsDir
  !if "${SIDEBARIMAGE}" != ""
    File "/oname=$PLUGINSDIR\marca-do-seele.bmp" "${SIDEBARIMAGE}"
  !endif

  Call SeeleLeAsEscolhas

  GetDlgItem $0 $HWNDPARENT 1034
  SetCtlColors $0 ${SEELE_OSSO} ${SEELE_PAINEL}
  GetDlgItem $0 $HWNDPARENT 1035
  SetCtlColors $0 ${SEELE_OSSO} ${SEELE_PAINEL}
  GetDlgItem $0 $HWNDPARENT 1036
  SetCtlColors $0 ${SEELE_LINHA} ${SEELE_PAINEL}
  GetDlgItem $0 $HWNDPARENT 1037
  SetCtlColors $0 ${SEELE_LARANJA} ${SEELE_PAINEL}
  SendMessage $0 ${WM_SETFONT} $SeeleFonteCartela 1
  GetDlgItem $0 $HWNDPARENT 1038
  SetCtlColors $0 ${SEELE_ROTULO} ${SEELE_PAINEL}
  GetDlgItem $0 $HWNDPARENT 1256
  SetCtlColors $0 ${SEELE_ROTULO} ${SEELE_NEGRO}
FunctionEnd

; A lombada: a marca e o que o SEELE é, à esquerda de cada página própria.
!macro SeeleLombada
  ${NSD_CreateBitmap} 0u 0u 40u 40u ""
  Pop $SeeleMarca
  ${NSD_SetImage} $SeeleMarca "$PLUGINSDIR\marca-do-seele.bmp" $R9

  ${NSD_CreateLabel} 0u 44u 60u 12u "SEELE"
  Pop $0
  SetCtlColors $0 ${SEELE_LARANJA} transparent
  SendMessage $0 ${WM_SETFONT} $SeeleFonteCartela 1

  ${NSD_CreateLabel} 0u 58u 62u 40u "VOZ, VÍDEO E TEXTO AUTO-HOSPEDADOS"
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent
  SendMessage $0 ${WM_SETFONT} $SeeleFonteRotulo 1

  ${NSD_CreateLabel} 0u 108u 62u 40u "O mesmo executável é o aplicativo e o servidor."
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent
!macroend

; ---------------------------------------------------------------- 01 DESTINO

Function SeelePaginaDestino
  !insertmacro MUI_HEADER_TEXT "PASSO 01 DE 04 · DESTINO" "Instalar o SEELE nesta máquina"

  nsDialogs::Create 1018
  Pop $SeeleDialogo
  ${If} $SeeleDialogo == error
    Abort
  ${EndIf}
  SetCtlColors $SeeleDialogo ${SEELE_OSSO} ${SEELE_NEGRO}

  !insertmacro SeeleLombada

  ${NSD_CreateLabel} 74u 0u 226u 26u "Nada é enviado para fora durante a instalação. O SEELE não cria conta: sua identidade é uma chave gerada aqui, no primeiro uso."
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent

  ${NSD_CreateLabel} 74u 34u 226u 8u "PASTA DE DESTINO"
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent
  SendMessage $0 ${WM_SETFONT} $SeeleFonteRotulo 1

  ${NSD_CreateText} 74u 44u 170u 12u "$INSTDIR"
  Pop $R0
  SetCtlColors $R0 ${SEELE_OSSO} ${SEELE_PAINEL}

  ${NSD_CreateButton} 250u 44u 50u 12u "ESCOLHER…"
  Pop $R1
  ${NSD_OnClick} $R1 SeeleEscolhePasta

  ${NSD_CreateLabel} 74u 64u 226u 8u "O SEELE ocupa cerca de 86 MiB depois de instalado."
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent

  ${NSD_CreateLabel} 74u 78u 226u 20u "Ao continuar você aceita a licença do projeto, que acompanha o executável."
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent

  nsDialogs::Show
FunctionEnd

Function SeeleEscolhePasta
  Pop $0
  nsDialogs::SelectFolderDialog "Onde instalar o SEELE" "$INSTDIR"
  Pop $1
  ${If} $1 != error
    StrCpy $INSTDIR $1
    ${NSD_SetText} $R0 $INSTDIR
  ${EndIf}
FunctionEnd

Function SeeleSaiDestino
  ${NSD_GetText} $R0 $0
  ${If} $0 != ""
    StrCpy $INSTDIR $0
  ${EndIf}
FunctionEnd

; ---------------------------------------------------------------- 02 OPÇÕES

Function SeelePaginaOpcoes
  !insertmacro MUI_HEADER_TEXT "PASSO 02 DE 04 · OPÇÕES" "O que o instalador vai mexer"

  nsDialogs::Create 1018
  Pop $SeeleDialogo
  ${If} $SeeleDialogo == error
    Abort
  ${EndIf}
  SetCtlColors $SeeleDialogo ${SEELE_OSSO} ${SEELE_NEGRO}

  !insertmacro SeeleLombada

  ${NSD_CreateLabel} 74u 0u 226u 18u "Os dois podem ser mudados depois: o atalho pela área de trabalho, a porta pelo firewall do Windows."
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent

  ${NSD_CreateCheckbox} 74u 24u 226u 10u "Atalho na área de trabalho"
  Pop $SeeleCaixaAtalho
  SetCtlColors $SeeleCaixaAtalho ${SEELE_OSSO} transparent
  ${If} $SeeleQuerAtalho == 1
    ${NSD_Check} $SeeleCaixaAtalho
  ${EndIf}

  ${NSD_CreateLabel} 84u 35u 216u 8u "e uma entrada no menu Iniciar"
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent

  ${NSD_CreateCheckbox} 74u 50u 226u 10u "Abrir a porta 8383 UDP no firewall do Windows"
  Pop $SeeleCaixaFirewall
  SetCtlColors $SeeleCaixaFirewall ${SEELE_OSSO} transparent
  ${If} $SeeleQuerFirewall == 1
    ${NSD_Check} $SeeleCaixaFirewall
  ${EndIf}

  ${NSD_CreateLabel} 84u 61u 216u 16u "Só é necessário se você for hospedar. Para entrar no servidor de outra pessoa, nada disso é preciso."
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent

  ${NSD_CreateLabel} 74u 84u 226u 16u "A regra é do programa, e não da porta solta: ela vale para o SEELE e para mais nada."
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent

  nsDialogs::Show
FunctionEnd

Function SeeleSaiOpcoes
  ${NSD_GetState} $SeeleCaixaAtalho $SeeleQuerAtalho
  ${NSD_GetState} $SeeleCaixaFirewall $SeeleQuerFirewall
FunctionEnd

; ---------------------------------------------------------------- 04 PRONTO

Function SeelePaginaPronto
  !insertmacro MUI_HEADER_TEXT "PASSO 04 DE 04 · PRONTO" "O SEELE está pronto"

  nsDialogs::Create 1018
  Pop $SeeleDialogo
  ${If} $SeeleDialogo == error
    Abort
  ${EndIf}
  SetCtlColors $SeeleDialogo ${SEELE_OSSO} ${SEELE_NEGRO}

  !insertmacro SeeleLombada

  ${NSD_CreateLabel} 74u 0u 226u 26u "Na primeira abertura o app gera sua chave e pede um apelido. Depois disso você escolhe entre entrar num servidor ou hospedar um aqui."
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent

  ${NSD_CreateLabel} 74u 32u 40u 8u "VERSÃO"
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent
  SendMessage $0 ${WM_SETFONT} $SeeleFonteRotulo 1
  ${NSD_CreateLabel} 120u 32u 180u 8u "${VERSION}"
  Pop $0
  SetCtlColors $0 ${SEELE_OSSO} transparent

  ${NSD_CreateLabel} 74u 44u 40u 8u "PASTA"
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent
  SendMessage $0 ${WM_SETFONT} $SeeleFonteRotulo 1
  ${NSD_CreateLabel} 120u 44u 180u 16u "$INSTDIR"
  Pop $0
  SetCtlColors $0 ${SEELE_OSSO} transparent

  ${NSD_CreateLabel} 74u 64u 40u 8u "PORTA UDP"
  Pop $0
  SetCtlColors $0 ${SEELE_ROTULO} transparent
  SendMessage $0 ${WM_SETFONT} $SeeleFonteRotulo 1
  ${If} $SeeleQuerFirewall == 1
    ${NSD_CreateLabel} 120u 64u 180u 16u "8383 aberta no firewall"
  ${Else}
    ${NSD_CreateLabel} 120u 64u 180u 16u "não aberta — você escolheu deixar fechada"
  ${EndIf}
  Pop $0
  SetCtlColors $0 ${SEELE_OSSO} transparent

  ${NSD_CreateCheckbox} 74u 86u 226u 10u "Abrir o SEELE agora"
  Pop $R2
  SetCtlColors $R2 ${SEELE_OSSO} transparent
  ${NSD_Check} $R2

  nsDialogs::Show
FunctionEnd

Function SeeleSaiPronto
  ${NSD_GetState} $R2 $0
  ${If} $0 == 1
    nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" ""
  ${EndIf}
FunctionEnd

; -------------------------------------------------------- a escolha que dura
;
; **O modo silencioso não tem quem responda, e é ele que aplica as
; atualizações.**
;
; Se a caixa do firewall nascesse desmarcada e a atualização rodasse sem tela, o
; gancho apagaria a regra de quem hospeda e não a poria de volta — e o servidor
; de alguém pararia de aceitar conexão numa atualização que ninguém pediu, sem
; nada na tela para explicar, porque tela é justamente o que não há.
;
; Então a escolha é gravada no registro e relida na próxima vez. Quem instalou
; antes desta versão não tem o valor gravado: para esses o padrão é **ligada**,
; que é como o instalador se comportava até aqui — mudar o padrão por baixo de
; quem já hospeda seria a mesma quebra por outro caminho.
!define SEELE_ESCOLHAS "Software\${MANUFACTURER}\${PRODUCTNAME}"

Function SeeleLeAsEscolhas
  ReadRegDWORD $SeeleQuerFirewall SHCTX "${SEELE_ESCOLHAS}" "FirewallUDP"
  ${If} ${Errors}
    StrCpy $SeeleQuerFirewall 1
    ClearErrors
  ${EndIf}
  ReadRegDWORD $SeeleQuerAtalho SHCTX "${SEELE_ESCOLHAS}" "AtalhoNaAreaDeTrabalho"
  ${If} ${Errors}
    StrCpy $SeeleQuerAtalho 1
    ClearErrors
  ${EndIf}
FunctionEnd
