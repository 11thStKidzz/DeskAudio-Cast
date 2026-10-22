export function transportControl({playing,busy,can_resume},action=false){
  return {glyph:playing?'⏸':'▶',disabled:Boolean(action||busy||(!playing&&!can_resume))};
}
