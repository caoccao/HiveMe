/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

// Placeholder entry point. Step 4.1 of docs/plans/plan-initialization.md replaces
// this with the real application: the MUI theme provider, the layout, and the
// Zustand store described in docs/specs/gui.md.

import React from 'react';
import ReactDOM from 'react-dom/client';

function Placeholder() {
  return (
    <main style={{ fontFamily: 'system-ui, sans-serif', padding: '2rem' }}>
      <h1>HiveMe</h1>
      <p>The GUI is scaffolded in step 4.1. See docs/specs/gui.md.</p>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Placeholder />
  </React.StrictMode>
);
