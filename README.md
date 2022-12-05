# rtic-twim-twis-dma-demo
A (working) example to demonstrate use of non-blocking DMA transactions on a nrf52840_mdk (makerdiary) board. 

The board houses 2 DMA-capable peripherals 

- TWIM (I2C in master mode) 
- TWIS (I2C in slave mode)

TWIM `reads` from and `writes` to TWIS via DMA asynchronously via `rtic`. 
