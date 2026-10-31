//! **A árvore do processo do jogo, no Windows** (revisão final, I9; decisão
//! W3 de `2026-10-31-api-8-windows-decisoes.md`): um *Job Object* com
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
//!
//! O filho de `rodar` entra na tarefa logo depois de nascer, e todo processo
//! que ele cria daí em diante nasce dentro dela (é a regra do Windows para
//! um processo numa tarefa). Matar a tarefa (`TerminateJobObject`) mata a
//! árvore inteira; e fechar a última alça dela — este [`Tarefa`] solto, ou o
//! app que cai — também, pelo `KILL_ON_JOB_CLOSE`. É o equivalente do grupo
//! de processos do Unix (`killpg`), e mais forte num ponto: o app que cai sem
//! soltar o `Programa` fecha as alças dele ao morrer, e o sistema mata a
//! tarefa sozinho (o custo (b) da decisão R8 não vale aqui).
//!
//! **O único `unsafe` do `seele-core` mora aqui** (o `Cargo.toml` diz por
//! quê). Cada bloco chama uma função do `kernel32` com alças que este módulo
//! criou e ainda não fechou, ou com a do filho, que o `tokio::process::Child`
//! segura aberta enquanto a vigia o tem.

#![cfg(windows)]
#![allow(unsafe_code)]

use std::os::windows::io::RawHandle;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

/// Uma tarefa do Windows com um processo dentro. Soltá-la mata o que restar
/// nela.
#[derive(Debug)]
pub(crate) struct Tarefa {
    alca: HANDLE,
}

// SAFETY: a alça de uma tarefa é um número do núcleo, sem estado da thread
// que a abriu; o Windows a aceita de qualquer thread.
unsafe impl Send for Tarefa {}
// SAFETY: as duas operações (`TerminateJobObject` e `CloseHandle`) são
// seguras de chamar de threads diferentes sobre a mesma alça.
unsafe impl Sync for Tarefa {}

impl Tarefa {
    /// Cria a tarefa e põe `processo` (a alça do filho) dentro.
    ///
    /// # Errors
    ///
    /// O que o Windows disse, em texto: a tarefa que não foi criada, o limite
    /// que não foi posto, ou o processo que não entrou.
    pub(crate) fn com(processo: RawHandle) -> Result<Self, String> {
        // SAFETY: sem atributos e sem nome — uma tarefa anônima, que só esta
        // alça alcança.
        let alca = unsafe { CreateJobObjectW(None, windows::core::PCWSTR::null()) }
            .map_err(|e| format!("CreateJobObjectW: {e}"))?;
        // Daqui em diante, um erro solta a tarefa (e o `Drop` fecha a alça).
        let tarefa = Self { alca };
        let mut limite = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limite.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let tamanho = u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| "o tamanho do limite não cabe num u32".to_owned())?;
        // SAFETY: `limite` vive até o fim desta chamada, e o tamanho é o dele.
        unsafe {
            SetInformationJobObject(
                tarefa.alca,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limite).cast(),
                tamanho,
            )
        }
        .map_err(|e| format!("SetInformationJobObject: {e}"))?;
        // SAFETY: `processo` é a alça do filho, aberta enquanto o `Child` que
        // a deu está vivo (quem chama o segura).
        unsafe { AssignProcessToJobObject(tarefa.alca, HANDLE(processo)) }
            .map_err(|e| format!("AssignProcessToJobObject: {e}"))?;
        Ok(tarefa)
    }

    /// Mata todo processo da tarefa.
    pub(crate) fn matar(&self) {
        // SAFETY: a alça é desta tarefa e só se fecha no `Drop`.
        match unsafe { TerminateJobObject(self.alca, 1) } {
            Ok(()) => tracing::debug!("programa: a tarefa do processo foi morta"),
            Err(erro) => {
                tracing::warn!(%erro, "programa: a tarefa do processo não morreu");
            }
        }
    }
}

impl Drop for Tarefa {
    fn drop(&mut self) {
        // O `KILL_ON_JOB_CLOSE` mata o que restar quando esta, a última alça,
        // fecha. `matar` antes não é preciso, mas deixa a ordem igual à do
        // Unix: a árvore morre aqui, e não «quando o núcleo fechar a alça».
        self.matar();
        // SAFETY: a alça é desta tarefa, e nada mais a usa depois do `Drop`.
        if let Err(erro) = unsafe { CloseHandle(self.alca) } {
            tracing::warn!(%erro, "programa: a alça da tarefa não fechou");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::System::JobObjects::IsProcessInJob;

    /// **Só roda no Windows.** O filho entra na tarefa, e matá-la o mata.
    #[test]
    fn o_filho_entra_na_tarefa_e_morre_com_ela() {
        let mut filho = std::process::Command::new("cmd")
            .args(["/C", "ping -n 30 127.0.0.1 > NUL"])
            .spawn()
            .unwrap();
        let tarefa = Tarefa::com(filho.as_raw_handle()).unwrap();
        let mut dentro = windows::core::BOOL(0);
        // SAFETY: as duas alças estão abertas.
        unsafe {
            IsProcessInJob(
                HANDLE(filho.as_raw_handle()),
                Some(tarefa.alca),
                &mut dentro,
            )
        }
        .unwrap();
        assert!(dentro.as_bool(), "o filho não entrou na tarefa");
        drop(tarefa);
        let estado = filho.wait().unwrap();
        assert!(
            !estado.success(),
            "o filho saiu sozinho, e não morto: {estado:?}"
        );
    }
}
