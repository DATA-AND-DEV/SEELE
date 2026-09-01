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
  nsExec::ExecToLog '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="SEELE" dir=in'
  Pop $0
  nsExec::ExecToLog '"$SYSDIR\netsh.exe" advfirewall firewall add rule name="SEELE" dir=in action=allow protocol=udp profile=any enable=yes program="$INSTDIR\${MAINBINARYNAME}.exe" description="Deixa entrar conexão para o servidor SEELE hospedado nesta máquina."'
  Pop $0
!macroend

; Sai junto com o programa. Numa atualização o desinstalador antigo roda antes
; do instalador novo, que recria a regra no gancho de cima — a ordem converge
; sozinha e não precisa perguntar se é atualização ou remoção de verdade.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="SEELE" dir=in'
  Pop $0
!macroend
