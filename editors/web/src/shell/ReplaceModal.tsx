import { exportDoc } from "../state/files.ts";
import { useStore } from "../state/store.ts";
import { Modal, modal } from "../ui/Modal.tsx";
import styles from "./ReplaceModal.module.css";

/** Asks before a new, imported, or dropped document replaces one with
 * changes not yet exported as `.mg`. */
export function ReplaceModal() {
    const replacing = useStore((s) => s.replacing);
    const fileName = useStore((s) => s.fileName);
    if (!replacing) return null;

    const store = useStore.getState();
    const cancel = () => store.setReplacing(null);
    const open = () => store.openDoc(replacing.fileName, replacing.text);

    return (
        <Modal
            title="Unexported changes"
            compact
            onClose={cancel}
            footer={
                <>
                    <span className={modal.hint} />
                    <button
                        type="button"
                        className={modal.button}
                        onClick={cancel}
                    >
                        Cancel
                    </button>
                    <button
                        type="button"
                        className={modal.button}
                        onClick={open}
                    >
                        Discard and open
                    </button>
                    <button
                        type="button"
                        className={`${modal.button} ${modal.primary}`}
                        // biome-ignore lint/a11y/noAutofocus: the safe choice takes Enter
                        autoFocus
                        onClick={() => {
                            exportDoc();
                            open();
                        }}
                    >
                        Export .mg, then open
                    </button>
                </>
            }
        >
            <p className={styles.message}>
                <strong>{fileName}</strong> has changes that haven't been
                exported. Opening <strong>{replacing.fileName}</strong> replaces
                it, and the changes can't be recovered.
            </p>
        </Modal>
    );
}
