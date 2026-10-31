- The FerroTERM Console moves to FerroUI 0.6.2 and `ferrohealth-server`
  0.4.2. A select in a dense options row and a long identifier or link in
  a stacked table no longer scroll the page sideways at 320 px, and the
  code editor keeps its typing speed as a JSON or XML document grows. The
  console's Content-Security-Policy is unchanged (`script-src 'self';
  style-src 'self'`, no nonce) and now comes from the family's
  server-rendered console headers, and FerroUI's script modules are served
  under `/ferrohealth-ui/0.6.2/`.
