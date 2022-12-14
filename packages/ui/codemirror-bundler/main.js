import { EditorView, basicSetup } from "codemirror"
import { EditorState, EditorSelection } from "@codemirror/state"
import { python } from "@codemirror/lang-python"

export function codemirror_new(doc) {
  return new EditorView({
    extensions: [
      basicSetup,
      EditorState.readOnly.of(true),
      python()
    ],
    doc,
  });
}

export function set_selection(view, from, to) {
  view.dispatch({
    selection: EditorSelection.create([EditorSelection.range(from, to)]),
    scrollIntoView: true,
  });
}
