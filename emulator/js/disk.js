export async function readFromDisk(start, end) {
  const {body} = await fetch('/disk', {
    headers: {
      'Content-Type': 'application/octet-stream',
      'Range': `bytes=${start}-${end}`
    }
  });
  const {value} = await body.getReader().read();
  return value;
}

export async function writeToDisk(startIdx, data) {
  await fetch('/disk', {
    method: 'PATCH',
    headers: {
      'Content-Type': 'application/octet-stream',
      'Range': `bytes=${startIdx}-${startIdx + data.length}`
    },
    body: data
  });
}
