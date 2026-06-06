function processUserData(user: any) {
  const name = user.name;
  const email = user.email;
  const age = user.age;
  const role = user.role;

  if (age < 18) {
    console.log(`${name} is a minor`);
    return { restricted: true };
  }

  const formatted = {
    displayName: name.toUpperCase(),
    contact: email.toLowerCase(),
    userRole: role,
    isAdult: age >= 18,
  };

  return formatted;
}
