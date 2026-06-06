function processProductData(product: any) {
  const title = product.title;
  const sku = product.sku;
  const price = product.price;
  const category = product.category;

  if (price < 10) {
    console.log(`${title} is discounted`);
    return { discount: true };
  }

  const result = {
    displayName: title.toUpperCase(),
    identifier: sku.toLowerCase(),
    itemCategory: category,
    isCheap: price < 10,
  };

  return result;
}
