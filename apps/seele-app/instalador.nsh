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
