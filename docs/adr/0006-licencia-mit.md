# ADR 0006: Licencia MIT para el repositorio

- **Estado:** Aceptado
- **Fecha:** 2026-10-15

## Contexto

El repositorio nació privado y sin licencia; la fundación dejó la decisión pendiente a pedido del
equipo. Es un proyecto de curso que con alta probabilidad se hará público como portafolio y para
que el docente lo revise sin invitaciones. Todas las dependencias del workspace y de `web/` son
permisivas (MIT, Apache-2.0, BSD, ISC, MPL-2.0, Zlib, CC0, CDLA-Permissive-2.0), así que ninguna
condiciona la elección. Como Taut es un SaaS, una licencia copyleft de red (AGPL) habría sido la
forma de impedir que alguien lo opere como servicio sin devolver sus cambios.

## Decisión

Licenciar todo el repositorio bajo **MIT**, con el aviso
`Copyright (c) 2026 Frank Rivera and Taut contributors` (en inglés, como el texto de la licencia).

Aplicación:

- `LICENSE` en la raíz con el texto oficial.
- `license = "MIT"` en `[workspace.package]` del `Cargo.toml` raíz; cada crate lo hereda con
  `license.workspace = true`. `deny.toml` deja de ignorar los crates privados: cargo-deny verifica
  también los nuestros.
- `"license": "MIT"` en `web/package.json`.
- Sección "Licencia" del `README.md` y "Licencia de las contribuciones" en `CONTRIBUTING.md`: todo
  PR se acepta bajo la misma licencia.

## Alternativas consideradas

- **Apache-2.0:** añade licencia explícita de patentes y términos de contribución; más formal y
  algo menos común en proyectos pequeños. Mientras haya un único titular, migrar de MIT a
  Apache-2.0 es trivial si el proyecto crece, así que no hace falta pagarlo hoy.
- **AGPL-3.0:** protege frente a servicios cerrados construidos sobre Taut, pero frena la adopción
  (muchas organizaciones la evitan) y no es el objetivo del curso; la referencia de producto
  (Uptime Kuma) usa MIT.
- **Sin licencia:** mientras el repo es privado no cambia nada, pero al hacerlo público nadie
  podría reutilizar el código, ni siquiera citarlo con ejemplos; y la decisión volvería a quedar
  pendiente.

## Consecuencias

- Cualquiera puede usar, modificar y redistribuir Taut conservando el aviso; también operarlo como
  servicio sin publicar cambios. Se asume: el valor del proyecto está en el aprendizaje y el
  portafolio, no en la exclusividad.
- Si el equipo crece, los nuevos autores quedan cubiertos por "Taut contributors"; no hace falta
  tocar el aviso.
- Hacer público el repositorio ya no depende de la licencia; sí de la decisión del equipo.
- Si en el futuro se quisiera cambiar a Apache-2.0 o AGPL, haría falta el acuerdo de todos los
  contribuidores con código en el repo: cuanto antes se decida, más barato es.
