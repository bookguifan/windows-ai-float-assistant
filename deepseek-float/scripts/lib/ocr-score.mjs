export function unwrapFence(text) {
  return String(text).trim().replace(/^```[^\n]*\n([\s\S]*?)\n```$/, '$1');
}
export function normalizeTranscript(text, kind='plain') {
  let value=unwrapFence(text).normalize('NFC');
  if(kind==='table') value=value.split(/\r?\n/).filter(line=>!/^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)*\|?\s*$/.test(line)).join('\n').replace(/\|/g,'');
  return value.replace(/\s/gu,'');
}
export function scoreTranscript(expected, actual, kind='plain') {
  const a=Array.from(normalizeTranscript(expected,kind));
  const b=Array.from(normalizeTranscript(actual,kind));
  let previous=Array.from({length:b.length+1},(_,i)=>i);
  for(let i=1;i<=a.length;i++) {
    const row=[i];
    for(let j=1;j<=b.length;j++) row.push(Math.min(row[j-1]+1,previous[j]+1,previous[j-1]+(a[i-1]===b[j-1]?0:1)));
    previous=row;
  }
  const edits=previous[b.length];
  return {edits,expectedCharacters:a.length,actualCharacters:b.length,characterErrorRate:a.length?edits/a.length:(b.length?1:0),exact:edits===0,
    linesPreserved:kind==='table'?null:unwrapFence(expected).split(/\r?\n/).map(x=>x.trim()).join('\n')===unwrapFence(actual).split(/\r?\n/).map(x=>x.trim()).join('\n')};
}
